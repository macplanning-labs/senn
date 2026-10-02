//! hub.rs — 部屋・接続・seq 採番・再送用リングバッファ
//!
//! 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §4, §5, §7.4
//!
//! - seq は Dispatcher（1本の経路）だけが、メモリ内で部屋ごとに振る。穴の無い連番になる。
//! - seq は「この epoch（= このプロセス）から、この部屋に何番目に送ったか」。台をまたぐ通し番号ではない。
//!   別の台につなぎ直すと epoch が変わるので、端末は番号を比べずに差分同期で揃える（正しさは崩れない）。
//! - 送信は待たない（try_send）。あふれた接続は切断し、端末の再接続（resume）に任せる。
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, RwLock};

use super::stats::{RoomSnapshot, Stats, StatsSnapshot};
use crate::domain::models::realtime::{Change, Origin, RoomId, RoomPosition, ServerPacket};

/// 部屋ごとに残す送信済みパケット数（短い切断ならここから再送する）
pub const RING_CAPACITY: usize = 512;
/// 1接続の送信待ちの上限。あふれたら切断する
pub const CONN_QUEUE: usize = 256;
/// 1人の利用者が同時に持てる接続の数。超えたら、いちばん古い接続を切る。
/// 接続は代表タブだけが張るので、通常は 1〜2（再接続の重なり）。暴走したクライアントやトークンの乱用への備え。
pub const MAX_CONNS_PER_USER: usize = 8;

/// 部屋ごとに1回だけ JSON にした本文。全購読者で共有する
pub type Frame = Arc<str>;
pub type ConnId = u64;

struct RoomState {
    seq: u64,
    ring: VecDeque<(u64, Frame)>,
    members: HashSet<ConnId>,
    /// 最後に、購読者が入った・出た・パケットが配られた時刻。誰もいない部屋を片付ける目安
    last_active: Instant,
}

impl RoomState {
    fn new() -> Self {
        Self {
            seq: 0,
            ring: VecDeque::new(),
            members: HashSet::new(),
            last_active: Instant::now(),
        }
    }
}

struct Conn {
    user_id: i32,
    tx: mpsc::Sender<Frame>,
    rooms: HashSet<RoomId>,
}

struct Inner {
    rooms: HashMap<RoomId, RoomState>,
    conns: HashMap<ConnId, Conn>,
    next_conn: ConnId,
}

pub struct Hub {
    pub epoch: String,
    /// 計測（管理者用 API・定期ログ）
    pub stats: Stats,
    inner: RwLock<Inner>,
}

fn to_frame(packet: &ServerPacket) -> Frame {
    // ServerPacket は文字列キーだけの構造体なので、シリアライズは失敗しない
    serde_json::to_string(packet)
        .expect("serialize ServerPacket")
        .into()
}

impl Hub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            epoch: uuid::Uuid::new_v4().to_string(),
            stats: Stats::default(),
            inner: RwLock::new(Inner {
                rooms: HashMap::new(),
                conns: HashMap::new(),
                next_conn: 1,
            }),
        })
    }

    /// 接続を登録し、購読した部屋の現在位置を返す
    pub async fn join(
        &self,
        user_id: i32,
        tx: mpsc::Sender<Frame>,
        rooms: Vec<RoomId>,
    ) -> (ConnId, Vec<RoomPosition>) {
        let mut inner = self.inner.write().await;
        self.enforce_user_cap(&mut inner, user_id);
        let cid = inner.next_conn;
        inner.next_conn += 1;
        inner.conns.insert(
            cid,
            Conn {
                user_id,
                tx,
                rooms: HashSet::new(),
            },
        );
        let positions = Self::apply_rooms(&mut inner, cid, rooms);
        (cid, positions)
    }

    /// 権限が変わったときの購読のやり直し。新しい購読一覧と位置を返す
    pub async fn set_rooms(&self, cid: ConnId, rooms: Vec<RoomId>) -> Vec<RoomPosition> {
        let mut inner = self.inner.write().await;
        if !inner.conns.contains_key(&cid) {
            return Vec::new();
        }
        Self::apply_rooms(&mut inner, cid, rooms)
    }

    /// 同じ利用者の接続が上限に達していたら、いちばん古い接続を切って空ける（新しい接続を優先する）
    fn enforce_user_cap(&self, inner: &mut Inner, user_id: i32) {
        loop {
            let mut mine: Vec<ConnId> = inner
                .conns
                .iter()
                .filter(|(_, c)| c.user_id == user_id)
                .map(|(id, _)| *id)
                .collect();
            if mine.len() < MAX_CONNS_PER_USER {
                return;
            }
            mine.sort_unstable();
            let oldest = mine[0];
            tracing::warn!(
                user_id,
                cid = oldest,
                "realtime: per-user connection cap reached, dropping the oldest connection"
            );
            self.stats.cap_evictions.fetch_add(1, Relaxed);
            Self::remove_conn(inner, oldest);
        }
    }

    fn apply_rooms(inner: &mut Inner, cid: ConnId, rooms: Vec<RoomId>) -> Vec<RoomPosition> {
        let new: HashSet<RoomId> = rooms.into_iter().collect();
        let old = inner
            .conns
            .get(&cid)
            .map(|c| c.rooms.clone())
            .unwrap_or_default();
        for gone in old.difference(&new) {
            if let Some(st) = inner.rooms.get_mut(gone) {
                st.members.remove(&cid);
                st.last_active = Instant::now();
            }
        }
        for added in new.difference(&old) {
            let st = inner
                .rooms
                .entry(added.clone())
                .or_insert_with(RoomState::new);
            st.members.insert(cid);
            st.last_active = Instant::now();
        }
        let mut positions: Vec<RoomPosition> = new
            .iter()
            .map(|r| RoomPosition {
                room: r.clone(),
                seq: inner.rooms.get(r).map_or(0, |st| st.seq),
            })
            .collect();
        positions.sort_by(|a, b| a.room.cmp(&b.room));
        if let Some(c) = inner.conns.get_mut(&cid) {
            c.rooms = new;
        }
        positions
    }

    pub async fn leave(&self, cid: ConnId) {
        let mut inner = self.inner.write().await;
        Self::remove_conn(&mut inner, cid);
    }

    fn remove_conn(inner: &mut Inner, cid: ConnId) {
        if let Some(conn) = inner.conns.remove(&cid) {
            for r in &conn.rooms {
                if let Some(st) = inner.rooms.get_mut(r) {
                    st.members.remove(&cid);
                    st.last_active = Instant::now();
                }
            }
        }
    }

    /// Dispatcher だけが呼ぶ。部屋の seq を 1 進めてパケットを作り、購読者に配る。
    pub async fn publish(&self, room: &RoomId, changes: Vec<Change>, origin: Option<Origin>) {
        let mut inner = self.inner.write().await;
        let (frame, members) = {
            let st = inner
                .rooms
                .entry(room.clone())
                .or_insert_with(RoomState::new);
            st.seq += 1;
            st.last_active = Instant::now();
            let frame = to_frame(&ServerPacket::Delta {
                epoch: self.epoch.clone(),
                room: room.clone(),
                seq: st.seq,
                changes,
                origin,
            });
            st.ring.push_back((st.seq, frame.clone()));
            while st.ring.len() > RING_CAPACITY {
                st.ring.pop_front();
            }
            (frame, st.members.iter().copied().collect::<Vec<_>>())
        };
        self.stats.published_packets.fetch_add(1, Relaxed);
        self.stats
            .published_bytes
            .fetch_add(frame.len() as u64, Relaxed);
        let mut slow = Vec::new();
        for cid in members {
            if let Some(c) = inner.conns.get(&cid) {
                if c.tx.try_send(frame.clone()).is_err() {
                    slow.push(cid);
                } else {
                    self.stats.deliveries.fetch_add(1, Relaxed);
                }
            }
        }
        // 送信待ちがあふれた接続は切る（tx を落とすと serve ループが終わる）。端末は再接続して resume する
        for cid in slow {
            tracing::warn!(cid, "realtime: slow consumer, dropping connection");
            self.stats.slow_drops.fetch_add(1, Relaxed);
            Self::remove_conn(&mut inner, cid);
        }
    }

    /// 接続の登録と再接続時の追いつきを、1つのロックの中で行う。
    /// 別々に行うと、登録と再送の間に来た新着が再送より先に並び、端末に順序の入れ替わりが見えてしまう。
    pub async fn join_resuming(
        &self,
        user_id: i32,
        tx: mpsc::Sender<Frame>,
        rooms: Vec<RoomId>,
        epoch: Option<&str>,
        from: &[RoomPosition],
    ) -> (ConnId, Vec<RoomPosition>) {
        let mut inner = self.inner.write().await;
        self.enforce_user_cap(&mut inner, user_id);
        let cid = inner.next_conn;
        inner.next_conn += 1;
        inner.conns.insert(
            cid,
            Conn {
                user_id,
                tx,
                rooms: HashSet::new(),
            },
        );
        let positions = Self::apply_rooms(&mut inner, cid, rooms);
        self.resume_locked(&mut inner, cid, epoch, from);
        (cid, positions)
    }

    /// 再接続時の追いつき（テスト・単独呼び出し用）。実際の接続は join_resuming を使う。
    pub async fn resume(&self, cid: ConnId, epoch: Option<&str>, from: &[RoomPosition]) {
        let mut inner = self.inner.write().await;
        self.resume_locked(&mut inner, cid, epoch, from);
    }

    /// 購読中の部屋ごとに、リングから再送できるなら再送し、できなければ resync を送る。
    /// 購読していない部屋の位置が渡されても無視する（権限の外は見えない）。
    fn resume_locked(
        &self,
        inner: &mut Inner,
        cid: ConnId,
        epoch: Option<&str>,
        from: &[RoomPosition],
    ) {
        let Some(conn) = inner.conns.get(&cid) else {
            return;
        };
        let tx = conn.tx.clone();
        let mut subscribed: Vec<RoomId> = conn.rooms.iter().cloned().collect();
        subscribed.sort();
        let same_epoch = epoch == Some(self.epoch.as_str());
        let mut frames: Vec<Frame> = Vec::new();
        for room in subscribed {
            let Some(st) = inner.rooms.get(&room) else {
                continue;
            };
            let known = from.iter().find(|p| p.room == room).map(|p| p.seq);
            let replay = match known {
                Some(last) if same_epoch && last <= st.seq => {
                    let covered = last == st.seq
                        || st.ring.front().is_some_and(|(first, _)| *first <= last + 1);
                    covered.then_some(last)
                }
                _ => None,
            };
            match replay {
                Some(last) => {
                    self.stats.resume_replays.fetch_add(1, Relaxed);
                    frames.extend(
                        st.ring
                            .iter()
                            .filter(|(s, _)| *s > last)
                            .map(|(_, f)| f.clone()),
                    )
                }
                None => {
                    self.stats.resume_resyncs.fetch_add(1, Relaxed);
                    frames.push(to_frame(&ServerPacket::Resync {
                        epoch: self.epoch.clone(),
                        room: room.clone(),
                        seq: st.seq,
                        reason: "buffer_exceeded".to_string(),
                        entities: vec![
                            "ticket".to_string(),
                            "project".to_string(),
                            "comment".to_string(),
                        ],
                    }))
                }
            }
        }
        for f in frames {
            if tx.try_send(f).is_err() {
                Self::remove_conn(inner, cid);
                return;
            }
        }
    }

    /// 全部屋に「差分同期で埋めてください」を送る（配信役の再起動・通知の取りこぼし・一括変更）。
    /// seq は進めない。端末は示された seq から続ける。
    pub async fn resync_all(&self, reason: &str, entities: &[&str]) {
        self.stats.count_resync(reason);
        let mut inner = self.inner.write().await;
        let mut targets: Vec<(ConnId, Frame)> = Vec::new();
        for (room, st) in inner.rooms.iter() {
            if st.members.is_empty() {
                continue;
            }
            let frame = to_frame(&ServerPacket::Resync {
                epoch: self.epoch.clone(),
                room: room.clone(),
                seq: st.seq,
                reason: reason.to_string(),
                entities: entities.iter().map(|e| e.to_string()).collect(),
            });
            targets.extend(st.members.iter().map(|cid| (*cid, frame.clone())));
        }
        let mut slow = Vec::new();
        for (cid, frame) in targets {
            if let Some(c) = inner.conns.get(&cid) {
                if c.tx.try_send(frame).is_err() {
                    slow.push(cid);
                }
            }
        }
        for cid in slow {
            Self::remove_conn(&mut inner, cid);
        }
    }

    /// 誰も購読していない部屋のうち、`ttl` より長く動きが無いものを片付ける（リングごと消す）。片付けた数を返す。
    ///
    /// 利用者ごとの部屋（`u:{id}`）や、使われなくなったチーム・プロジェクトの部屋が、接続が切れたあとも
    /// 残り続けるのを防ぐ。購読者のいる部屋は、静かでも消さない。
    /// 消した部屋に、あとで誰かが入り直すと seq は 0 から数え直しになる。端末が覚えている位置のほうが進んでいるので、
    /// 再接続のときは再送ではなく resync になり、端末は示された seq に合わせ直す（取りこぼさない）。
    pub async fn prune_idle_rooms(&self, ttl: Duration) -> usize {
        let mut inner = self.inner.write().await;
        let now = Instant::now();
        let before = inner.rooms.len();
        inner.rooms.retain(|_, st| {
            !st.members.is_empty() || now.saturating_duration_since(st.last_active) < ttl
        });
        before - inner.rooms.len()
    }

    /// 接続中の利用者 id（重複なし）
    pub async fn connected_users(&self) -> Vec<i32> {
        let inner = self.inner.read().await;
        let set: HashSet<i32> = inner.conns.values().map(|c| c.user_id).collect();
        set.into_iter().collect()
    }

    /// 利用者の接続 id と、その購読中の部屋
    pub async fn conns_of_user(&self, user_id: i32) -> Vec<(ConnId, HashSet<RoomId>)> {
        let inner = self.inner.read().await;
        inner
            .conns
            .iter()
            .filter(|(_, c)| c.user_id == user_id)
            .map(|(id, c)| (*id, c.rooms.clone()))
            .collect()
    }

    /// 1接続にだけ送る（権限の変更の通知など）。あふれたら切断する
    pub async fn send_to(&self, cid: ConnId, packet: &ServerPacket) {
        let mut inner = self.inner.write().await;
        let frame = to_frame(packet);
        let failed = inner
            .conns
            .get(&cid)
            .is_some_and(|c| c.tx.try_send(frame).is_err());
        if failed {
            Self::remove_conn(&mut inner, cid);
        }
    }

    /// 管理者用 API・定期ログ用の現在の状態。リングの本文は数えるだけ（コピーしない）
    pub async fn snapshot(&self) -> StatsSnapshot {
        let inner = self.inner.read().await;
        let users: HashSet<i32> = inner.conns.values().map(|c| c.user_id).collect();
        let mut rooms: Vec<RoomSnapshot> = inner
            .rooms
            .iter()
            .map(|(id, st)| RoomSnapshot {
                room: id.0.clone(),
                members: st.members.len(),
                ring_bytes: st.ring.iter().map(|(_, f)| f.len()).sum(),
            })
            .collect();
        let ring_packets: usize = inner.rooms.values().map(|s| s.ring.len()).sum();
        let ring_bytes: usize = rooms.iter().map(|r| r.ring_bytes).sum();
        rooms.sort_by(|a, b| {
            b.ring_bytes
                .cmp(&a.ring_bytes)
                .then(b.members.cmp(&a.members))
                .then(a.room.cmp(&b.room))
        });
        rooms.truncate(5);
        StatsSnapshot {
            epoch: self.epoch.clone(),
            connections: inner.conns.len(),
            users: users.len(),
            rooms: inner.rooms.len(),
            ring_packets,
            ring_bytes,
            top_rooms: rooms,
            totals: self.stats.totals(),
            dispatch_latency: self.stats.dispatch_latency(),
            ping_roundtrip: self.stats.ping_roundtrip(),
        }
    }

    /// 接続数（監視・テスト用）
    pub async fn connection_count(&self) -> usize {
        self.inner.read().await.conns.len()
    }

    /// 接続に紐づく購読一覧
    pub async fn rooms_of(&self, cid: ConnId) -> Vec<RoomId> {
        let inner = self.inner.read().await;
        let mut v: Vec<RoomId> = inner
            .conns
            .get(&cid)
            .map(|c| c.rooms.iter().cloned().collect())
            .unwrap_or_default();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upsert(id: i64) -> Vec<Change> {
        vec![Change::Stale {
            entity: "ticket".into(),
            id,
            v: 1,
        }]
    }

    async fn recv_all(rx: &mut mpsc::Receiver<Frame>) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        while let Ok(f) = rx.try_recv() {
            out.push(serde_json::from_str(&f).unwrap());
        }
        out
    }

    #[tokio::test]
    async fn seq_is_gapless_per_room_and_only_members_receive() {
        let hub = Hub::new();
        let (tx_a, mut rx_a) = mpsc::channel(16);
        let (tx_b, mut rx_b) = mpsc::channel(16);
        let (_a, _) = hub.join(1, tx_a, vec![RoomId::team(1)]).await;
        let (_b, _) = hub.join(2, tx_b, vec![RoomId::team(2)]).await;

        for i in 0..3 {
            hub.publish(&RoomId::team(1), upsert(i), None).await;
        }
        hub.publish(&RoomId::team(2), upsert(9), None).await;

        let a = recv_all(&mut rx_a).await;
        assert_eq!(
            a.iter()
                .map(|p| p["seq"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(a.iter().all(|p| p["room"] == "t:1"));
        let b = recv_all(&mut rx_b).await;
        assert_eq!(b.len(), 1);
        assert_eq!(b[0]["seq"], 1);
    }

    #[tokio::test]
    async fn resume_replays_from_ring_in_order() {
        let hub = Hub::new();
        let (tx, mut rx) = mpsc::channel(64);
        let (cid, _) = hub.join(1, tx, vec![RoomId::team(1)]).await;
        for i in 0..5 {
            hub.publish(&RoomId::team(1), upsert(i), None).await;
        }
        let _ = recv_all(&mut rx).await;

        hub.resume(
            cid,
            Some(&hub.epoch),
            &[RoomPosition {
                room: RoomId::team(1),
                seq: 2,
            }],
        )
        .await;
        let got = recv_all(&mut rx).await;
        assert_eq!(
            got.iter()
                .map(|p| p["seq"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![3, 4, 5]
        );
        assert!(got.iter().all(|p| p["type"] == "delta"));
    }

    #[tokio::test]
    async fn resume_sends_resync_when_ring_does_not_cover_or_epoch_differs() {
        let hub = Hub::new();
        let (tx, mut rx) = mpsc::channel(1024);
        let (cid, _) = hub.join(1, tx, vec![RoomId::team(1)]).await;
        for i in 0..(RING_CAPACITY as i64 + 10) {
            hub.publish(&RoomId::team(1), upsert(i), None).await;
        }
        let _ = recv_all(&mut rx).await;
        let latest = RING_CAPACITY as u64 + 10;

        // リングの先頭より古い位置 → resync
        hub.resume(
            cid,
            Some(&hub.epoch),
            &[RoomPosition {
                room: RoomId::team(1),
                seq: 3,
            }],
        )
        .await;
        let got = recv_all(&mut rx).await;
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["type"], "resync");
        assert_eq!(got[0]["seq"], latest);

        // epoch が違う → resync
        hub.resume(
            cid,
            Some("other-epoch"),
            &[RoomPosition {
                room: RoomId::team(1),
                seq: latest,
            }],
        )
        .await;
        assert_eq!(recv_all(&mut rx).await[0]["type"], "resync");

        // 位置を知らない部屋（初回接続） → resync
        hub.resume(cid, None, &[]).await;
        assert_eq!(recv_all(&mut rx).await[0]["type"], "resync");

        // 最新と同じ位置 → 何も送らない
        hub.resume(
            cid,
            Some(&hub.epoch),
            &[RoomPosition {
                room: RoomId::team(1),
                seq: latest,
            }],
        )
        .await;
        assert!(recv_all(&mut rx).await.is_empty());
    }

    /// 購読していない部屋の位置を送っても、その部屋の中身は返らない（権限の外は見えない）
    #[tokio::test]
    async fn resume_never_leaks_unsubscribed_rooms() {
        let hub = Hub::new();
        let (tx_other, _rx_other) = mpsc::channel(16);
        let (_o, _) = hub.join(9, tx_other, vec![RoomId::team(2)]).await;
        hub.publish(&RoomId::team(2), upsert(1), None).await;

        let (tx, mut rx) = mpsc::channel(16);
        let (cid, _) = hub.join(1, tx, vec![RoomId::team(1)]).await;
        hub.resume(
            cid,
            Some(&hub.epoch),
            &[RoomPosition {
                room: RoomId::team(2),
                seq: 0,
            }],
        )
        .await;
        let got = recv_all(&mut rx).await;
        assert!(
            got.iter().all(|p| p["room"] != "t:2"),
            "他チームの部屋が漏れた: {got:?}"
        );
    }

    #[tokio::test]
    async fn slow_consumer_is_dropped_and_others_keep_receiving() {
        let hub = Hub::new();
        let (tx_slow, _rx_slow) = mpsc::channel(1); // 読まない・小さい
        let (tx_ok, mut rx_ok) = mpsc::channel(16);
        hub.join(3, tx_slow, vec![RoomId::team(1)]).await;
        hub.join(4, tx_ok, vec![RoomId::team(1)]).await;
        for i in 0..3 {
            hub.publish(&RoomId::team(1), upsert(i), None).await;
        }
        assert_eq!(hub.connection_count().await, 1);
        assert_eq!(recv_all(&mut rx_ok).await.len(), 3);
    }

    #[tokio::test]
    async fn set_rooms_moves_membership() {
        let hub = Hub::new();
        let (tx, mut rx) = mpsc::channel(16);
        let (cid, _) = hub.join(1, tx, vec![RoomId::team(1)]).await;
        let pos = hub.set_rooms(cid, vec![RoomId::team(2)]).await;
        assert_eq!(pos.len(), 1);
        assert_eq!(hub.rooms_of(cid).await, vec![RoomId::team(2)]);
        hub.publish(&RoomId::team(1), upsert(1), None).await;
        hub.publish(&RoomId::team(2), upsert(2), None).await;
        let got = recv_all(&mut rx).await;
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["room"], "t:2");
    }

    #[tokio::test]
    async fn resync_all_reaches_members_without_advancing_seq() {
        let hub = Hub::new();
        let (tx, mut rx) = mpsc::channel(16);
        hub.join(1, tx, vec![RoomId::team(1)]).await;
        hub.publish(&RoomId::team(1), upsert(1), None).await;
        let _ = recv_all(&mut rx).await;
        hub.resync_all("bulk_change", &["ticket"]).await;
        let got = recv_all(&mut rx).await;
        assert_eq!(got[0]["type"], "resync");
        assert_eq!(got[0]["seq"], 1);
        assert_eq!(got[0]["reason"], "bulk_change");
    }

    #[tokio::test]
    async fn per_user_cap_drops_the_oldest_connection_and_keeps_the_newest() {
        let hub = Hub::new();
        let mut rxs = Vec::new();
        for _ in 0..(MAX_CONNS_PER_USER + 2) {
            let (tx, rx) = mpsc::channel(16);
            hub.join(1, tx, vec![RoomId::team(1)]).await;
            rxs.push(rx);
        }
        assert_eq!(
            hub.connection_count().await,
            MAX_CONNS_PER_USER,
            "上限を超えない"
        );
        assert_eq!(hub.stats.totals().cap_evictions, 2);

        hub.publish(&RoomId::team(1), upsert(1), None).await;
        // いちばん古い2本は切られている（送信側が落とされ、受信は終わる）
        assert!(rxs[0].recv().await.is_none() && rxs[1].recv().await.is_none());
        // 新しい接続は届く
        assert_eq!(recv_all(rxs.last_mut().unwrap()).await.len(), 1);
    }

    #[tokio::test]
    async fn the_cap_is_per_user_not_global() {
        let hub = Hub::new();
        let mut keep = Vec::new();
        for user in 1..=(MAX_CONNS_PER_USER as i32 + 3) {
            let (tx, rx) = mpsc::channel(16);
            hub.join(user, tx, vec![RoomId::team(1)]).await;
            keep.push(rx);
        }
        assert_eq!(
            hub.connection_count().await,
            MAX_CONNS_PER_USER + 3,
            "別々の利用者は、それぞれ上限まで持てる"
        );
        assert_eq!(hub.stats.totals().cap_evictions, 0);
    }

    #[tokio::test]
    async fn snapshot_reports_connections_rooms_ring_memory_and_totals() {
        let hub = Hub::new();
        let (tx_a, mut rx_a) = mpsc::channel(16);
        let (tx_b, _rx_b) = mpsc::channel(1); // 読まない・小さい → あふれる
        hub.join(1, tx_a, vec![RoomId::team(1)]).await;
        hub.join(2, tx_b, vec![RoomId::team(1), RoomId::team(2)])
            .await;
        for i in 0..3 {
            hub.publish(&RoomId::team(1), upsert(i), None).await;
        }
        hub.publish(&RoomId::team(2), upsert(9), None).await;
        let _ = recv_all(&mut rx_a).await;

        let s = hub.snapshot().await;
        assert_eq!(s.epoch, hub.epoch);
        assert_eq!(s.connections, 1, "あふれた接続は切られている");
        assert_eq!(s.users, 1);
        assert_eq!(s.rooms, 2);
        assert_eq!(s.ring_packets, 4);
        assert!(s.ring_bytes > 0);
        assert_eq!(s.top_rooms[0].room, "t:1", "リングが大きい部屋が先頭");
        assert_eq!(s.top_rooms[0].members, 1);
        assert_eq!(s.totals.published_packets, 4);
        assert!(s.totals.published_bytes > 0);
        assert_eq!(s.totals.slow_drops, 1);
        assert!(
            s.totals.deliveries >= 3,
            "配信できた分だけ数える: {}",
            s.totals.deliveries
        );
    }

    #[tokio::test]
    async fn resume_and_resync_are_counted() {
        let hub = Hub::new();
        let (tx, mut rx) = mpsc::channel(64);
        let (cid, _) = hub.join(1, tx, vec![RoomId::team(1)]).await;
        hub.publish(&RoomId::team(1), upsert(1), None).await;
        let _ = recv_all(&mut rx).await;
        hub.resume(
            cid,
            Some(&hub.epoch),
            &[RoomPosition {
                room: RoomId::team(1),
                seq: 0,
            }],
        )
        .await; // 再送できる
        hub.resume(cid, None, &[]).await; // 位置を知らない → resync
        hub.resync_all("bulk_change", &["ticket"]).await;
        let t = hub.stats.totals();
        assert_eq!(
            (t.resume_replays, t.resume_resyncs, t.resync_bulk_change),
            (1, 1, 1)
        );
    }

    #[tokio::test]
    async fn idle_rooms_without_members_are_pruned_after_the_ttl_but_active_ones_are_kept() {
        let hub = Hub::new();
        let (tx, _rx) = mpsc::channel(16);
        let (cid, _) = hub
            .join(1, tx, vec![RoomId::user(1), RoomId::team(1)])
            .await;
        hub.publish(&RoomId::user(1), upsert(1), None).await;
        hub.publish(&RoomId::team(7), upsert(2), None).await; // 誰も入っていない部屋
        assert_eq!(hub.snapshot().await.rooms, 3);

        // まだ新しい: 消さない
        assert_eq!(
            hub.prune_idle_rooms(std::time::Duration::from_secs(600))
                .await,
            0
        );
        // 十分に古い（ttl 0 = すぐ期限切れ）: 誰もいない部屋だけ消す。購読者のいる部屋は、静かでも残す
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(
            hub.prune_idle_rooms(std::time::Duration::from_millis(1))
                .await,
            1
        );
        let s = hub.snapshot().await;
        assert_eq!(s.rooms, 2);
        assert!(s.top_rooms.iter().all(|r| r.room != "t:7"));

        // 接続が切れると、その部屋も片付けの対象になる
        hub.leave(cid).await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(
            hub.prune_idle_rooms(std::time::Duration::from_millis(1))
                .await,
            2
        );
        assert_eq!(hub.snapshot().await.rooms, 0);
    }

    /// 部屋を片付けたあとに同じ部屋に入り直しても、端末は再同期で位置を合わせ直すので、更新を取りこぼさない
    #[tokio::test]
    async fn a_pruned_room_can_be_rejoined_and_the_client_is_told_to_resync() {
        let hub = Hub::new();
        let (tx, mut rx) = mpsc::channel(16);
        let (cid, _) = hub.join(1, tx, vec![RoomId::team(1)]).await;
        for i in 0..3 {
            hub.publish(&RoomId::team(1), upsert(i), None).await;
        }
        let _ = recv_all(&mut rx).await;
        hub.leave(cid).await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(
            hub.prune_idle_rooms(std::time::Duration::from_millis(1))
                .await,
            1
        );

        // 端末は seq=3 を覚えている。部屋は作り直されて seq=0 → 再送はできず、resync（seq=0）が返る
        let (tx2, mut rx2) = mpsc::channel(16);
        hub.join_resuming(
            1,
            tx2,
            vec![RoomId::team(1)],
            Some(&hub.epoch),
            &[RoomPosition {
                room: RoomId::team(1),
                seq: 3,
            }],
        )
        .await;
        let got = recv_all(&mut rx2).await;
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0]["type"].as_str(), got[0]["seq"].as_u64()),
            (Some("resync"), Some(0))
        );
    }
}
