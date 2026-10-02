//! dispatcher.rs — DB の変更通知（pg_notify）を、部屋ごとの差分パケットに変えて Hub に配る
//!
//! 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §7.1, §7.5
//!
//! 流れ: 通知を受ける → 30ms まとめる → 同じ行は最後の1件にまとめる → 実データを取り直す
//!       → 行がどの部屋に属するかを決める → 部屋ごとに Hub.publish（ここで seq が振られる）
//!
//! 順序について: `sync_version` は行ごとの番号なので、別の行どうしを並べる意味は無い。守るのは
//! 「同じ行について、後の seq で古い版を送らない」こと。(1) 同じ行は1まとまり1件、(2) 取り直しは常にその時点の
//! コミット済みの最新なので後のまとまりほど版は同じか新しい、(3) 端末は版で比べて古いものを捨てる、の三重で守る。
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use sqlx::postgres::PgListener;
use sqlx::PgPool;
use tokio::sync::mpsc;

use super::hub::Hub;
use super::rooms::{global_room, rooms_for_access, rooms_for_project, rooms_for_ticket};
use crate::domain::models::realtime::{Change, RoomId, ServerPacket};
use crate::infrastructure::repositories::sync_repo::{self, CommentPushRow, TicketPushRow};

/// この時間内の通知は1回にまとめる
const COALESCE: Duration = Duration::from_millis(30);
/// 1回のまとまりがこの件数を超えたら、個別に送らず差分同期に切り替える
const BULK_THRESHOLD: usize = 500;
/// 1行の本文がこれを超えたら実データは送らず stale にする
const MAX_ROW_BYTES: usize = 64 * 1024;
/// 期間限定メンバーの期限切れは時間で変わるので、定期的に購読を見直す
const ACCESS_RECHECK: Duration = Duration::from_secs(600);
/// 誰も購読していない部屋を、これだけ動きが無ければ片付ける
const IDLE_ROOM_TTL: Duration = Duration::from_secs(600);
/// 期限切れの接続用トークンの掃除間隔
const TOKEN_SWEEP: Duration = Duration::from_secs(60);

/// pg_notify('senn_sync') の本文（トリガー sync_notify_* が作る）
#[derive(Debug, Clone, Deserialize)]
pub struct Notice {
    pub e: String,
    pub a: String,
    pub id: i64,
    #[serde(default)]
    pub v: i64,
    pub t: Option<i32>,
    pub p: Option<i32>,
    pub ot: Option<i32>,
    pub opj: Option<i32>,
    /// 通知の宛先（notification）
    #[serde(default)]
    pub u: Option<i32>,
    /// プロジェクトの所属チーム（cycle / wiki）
    #[serde(default)]
    pub teams: Vec<i32>,
}

/// 同じ行の通知を1つにまとめたもの
#[derive(Debug, Clone)]
pub struct Merged {
    pub entity: String,
    pub id: i64,
    pub last_action: String,
    pub v: i64,
    /// この行が過去〜現在にいた（チーム, プロジェクト）。移動前の部屋への evict に使う
    pub places: BTreeSet<(Option<i32>, Option<i32>)>,
    /// 合図（signal）の流し先。端末内 DB に表を持たない対象（添付・リアクション・サイクル・通知・Wiki）で使う
    pub signal_rooms: BTreeSet<RoomId>,
}

/// 実データではなく「変わった」という合図だけを流す種別
pub const SIGNAL_ENTITIES: [&str; 5] = ["attachment", "reaction", "cycle", "notification", "wiki"];

fn is_signal(entity: &str) -> bool {
    SIGNAL_ENTITIES.contains(&entity)
}

/// 合図の流し先の部屋
fn signal_rooms_of(n: &Notice) -> BTreeSet<RoomId> {
    match n.e.as_str() {
        // 添付・リアクションは親チケット、サイクルはチームに属する（プロジェクトは任意）。どれもチケットと同じ規則
        "attachment" | "reaction" | "cycle" => rooms_for_ticket(n.t, n.p),
        // Wiki: チーム指定があればチケットと同じ規則、なければプロジェクト（所属チーム）、どちらも無ければ全員
        "wiki" if n.t.is_some() => rooms_for_ticket(n.t, n.p),
        "wiki" => rooms_for_project(n.p, &n.teams),
        "notification" => {
            n.u.map(|u| BTreeSet::from([RoomId::user(u)]))
                .unwrap_or_default()
        }
        _ => BTreeSet::new(),
    }
}

/// 親が先に届くように、プロジェクト → チケット → コメントの順に並べる
fn order_of(entity: &str) -> u8 {
    match entity {
        "project" => 0,
        "ticket" => 1,
        "comment" => 2,
        _ => 3, // 合図は実データの後
    }
}

/// 同じ行は最後の1件にまとめる。プロジェクトを先、チケットを後に並べる（親が先に届くように）
pub fn merge(notices: Vec<Notice>) -> Vec<Merged> {
    let mut by_key: HashMap<(String, i64), Merged> = HashMap::new();
    let mut order: Vec<(String, i64)> = Vec::new();
    for n in notices {
        let key = (n.e.clone(), n.id);
        let m = by_key.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Merged {
                entity: n.e.clone(),
                id: n.id,
                last_action: n.a.clone(),
                v: n.v,
                places: BTreeSet::new(),
                signal_rooms: BTreeSet::new(),
            }
        });
        m.last_action = n.a.clone();
        m.v = m.v.max(n.v);
        if is_signal(&n.e) {
            m.signal_rooms.extend(signal_rooms_of(&n));
        }
        if n.e == "ticket" || n.e == "comment" {
            m.places.insert((n.t, n.p));
            if n.ot.is_some() || n.opj.is_some() {
                m.places.insert((n.ot.or(n.t), n.opj.or(n.p)));
            }
        }
    }
    let mut out: Vec<Merged> = order
        .into_iter()
        .filter_map(|k| by_key.remove(&k))
        .collect();
    out.sort_by_key(|m| order_of(&m.entity)); // 安定ソート
    out
}

fn union_rooms<'a>(
    places: impl IntoIterator<Item = &'a (Option<i32>, Option<i32>)>,
) -> BTreeSet<RoomId> {
    places
        .into_iter()
        .flat_map(|(t, p)| rooms_for_ticket(*t, *p))
        .collect()
}

/// 通知のまとまりと取り直した行から、部屋ごとの Change を作る（DB を触らない純粋な処理）
pub fn plan(
    merged: &[Merged],
    rows: &HashMap<i64, TicketPushRow>,
    comment_rows: &HashMap<i64, CommentPushRow>,
) -> BTreeMap<RoomId, Vec<Change>> {
    let mut per_room: BTreeMap<RoomId, Vec<Change>> = BTreeMap::new();
    let mut push = |rooms: BTreeSet<RoomId>, change: Change| {
        for r in rooms {
            per_room.entry(r).or_default().push(change.clone());
        }
    };
    for m in merged {
        match m.entity.as_str() {
            "project" => {
                // 全員が見られる。DTO に利用者ごとの項目（is_member 等）があるので実データは送らない
                let change = if m.last_action == "delete" {
                    Change::Delete {
                        entity: "project".into(),
                        id: m.id,
                        v: m.v,
                    }
                } else {
                    Change::Stale {
                        entity: "project".into(),
                        id: m.id,
                        v: m.v,
                    }
                };
                push(BTreeSet::from([global_room()]), change);
            }
            "ticket" => {
                if m.last_action == "delete" {
                    push(
                        union_rooms(&m.places),
                        Change::Delete {
                            entity: "ticket".into(),
                            id: m.id,
                            v: m.v,
                        },
                    );
                    continue;
                }
                // 取り直す前に消えた行は、あとから delete の通知が来る
                let Some(row) = rows.get(&m.id) else { continue };
                let current = rooms_for_ticket(row.team_id, row.project_id);
                let data = serde_json::to_value(&row.ticket).unwrap_or(serde_json::Value::Null);
                let too_big = data.to_string().len() > MAX_ROW_BYTES;
                let change = if too_big {
                    Change::Stale {
                        entity: "ticket".into(),
                        id: m.id,
                        v: row.ticket.v,
                    }
                } else {
                    Change::Upsert {
                        entity: "ticket".into(),
                        id: m.id,
                        v: row.ticket.v,
                        data,
                    }
                };
                let moved_out: BTreeSet<RoomId> = union_rooms(&m.places)
                    .difference(&current)
                    .cloned()
                    .collect();
                push(current, change);
                push(
                    moved_out,
                    Change::Evict {
                        entity: "ticket".into(),
                        id: m.id,
                        v: row.ticket.v,
                    },
                );
            }
            "comment" => {
                if m.last_action == "delete" {
                    push(
                        union_rooms(&m.places),
                        Change::Delete {
                            entity: "comment".into(),
                            id: m.id,
                            v: m.v,
                        },
                    );
                    continue;
                }
                let Some(row) = comment_rows.get(&m.id) else {
                    continue;
                };
                // コメントは親チケットの部屋に流す。チケットが移動したときは、コメントの版も進むので
                // 通常の更新として移動先に届く（元の部屋の端末は、チケットの evict と一緒にコメントも消す）
                let rooms = rooms_for_ticket(row.team_id, row.project_id);
                let data = serde_json::to_value(&row.comment).unwrap_or(serde_json::Value::Null);
                let change = if data.to_string().len() > MAX_ROW_BYTES {
                    Change::Stale {
                        entity: "comment".into(),
                        id: m.id,
                        v: row.comment.v,
                    }
                } else {
                    Change::Upsert {
                        entity: "comment".into(),
                        id: m.id,
                        v: row.comment.v,
                        data,
                    }
                };
                push(rooms, change);
            }
            e if is_signal(e) => {
                // 合図: 種別と id だけ。通知は本人の部屋に何度も同じ合図が積まれないよう、id を 0 にまとめる
                let id = if e == "notification" { 0 } else { m.id };
                push(
                    m.signal_rooms.clone(),
                    Change::Stale {
                        entity: e.to_string(),
                        id,
                        v: 0,
                    },
                );
            }
            _ => {}
        }
    }
    // 同じ合図の重複（一括既読で通知が何十件も変わったときなど）は1つにまとめる
    for changes in per_room.values_mut() {
        let mut seen = BTreeSet::new();
        changes.retain(|c| match c {
            Change::Stale { entity, id, .. } if is_signal(entity) => {
                seen.insert((entity.clone(), *id))
            }
            _ => true,
        });
    }
    per_room
}

/// 1回のまとまりを処理して Hub に配る
pub async fn dispatch(
    pool: &PgPool,
    hub: &Hub,
    notices: Vec<Notice>,
    ai_agent_username: &str,
) -> anyhow::Result<()> {
    let merged = merge(notices);
    if merged.len() > BULK_THRESHOLD {
        hub.resync_all("bulk_change", &["ticket", "project", "comment"])
            .await;
        return Ok(());
    }
    // 合図だけのまとまりは、DB を読む必要が無い（下の取り直しは対象が無ければ何もしない）
    let ticket_ids: Vec<i64> = merged
        .iter()
        .filter(|m| m.entity == "ticket" && m.last_action != "delete")
        .map(|m| m.id)
        .collect();
    let rows: HashMap<i64, TicketPushRow> = sync_repo::fetch_ticket_push_rows(pool, &ticket_ids)
        .await?
        .into_iter()
        .map(|r| (r.ticket.base.id as i64, r))
        .collect();
    let comment_ids: Vec<i64> = merged
        .iter()
        .filter(|m| m.entity == "comment" && m.last_action != "delete")
        .map(|m| m.id)
        .collect();
    let comment_rows: HashMap<i64, CommentPushRow> =
        sync_repo::fetch_comment_push_rows(pool, &comment_ids, ai_agent_username)
            .await?
            .into_iter()
            .map(|r| (r.comment.id as i64, r))
            .collect();
    for (room, changes) in plan(&merged, &rows, &comment_rows) {
        hub.publish(&room, changes, None).await;
    }
    Ok(())
}

/// 利用者の購読を計算し直し、変わっていれば接続に access パケットを送る
pub async fn recompute_access(pool: &PgPool, hub: &Hub, user_id: i32) -> anyhow::Result<()> {
    let conns = hub.conns_of_user(user_id).await;
    if conns.is_empty() {
        return Ok(());
    }
    // 無効化されたユーザー(on)は、購読をすべて外し、見える範囲を空にする(端末は行を消す)
    let (access, rooms) = match sync_repo::realtime_access(pool, user_id).await? {
        Some(access) => {
            let rooms = rooms_for_access(&access, user_id);
            (access, rooms)
        }
        None => (
            crate::domain::models::sync_api::SyncAccessOut {
                all: false,
                team_ids: vec![],
                scoped_projects: vec![],
            },
            vec![],
        ),
    };
    let wanted: std::collections::HashSet<RoomId> = rooms.iter().cloned().collect();
    for (cid, current) in conns {
        if current == wanted {
            continue;
        }
        let positions = hub.set_rooms(cid, rooms.clone()).await;
        hub.send_to(
            cid,
            &ServerPacket::Access {
                access: access.clone(),
                rooms: positions,
            },
        )
        .await;
    }
    Ok(())
}

enum Event {
    Sync(Notice),
    /// 購読の再計算。Some = そのユーザー、None = 接続中の全員(公開区分の変更。E-3)
    Access(Option<i32>),
    /// LISTEN の接続が切れた。切れている間の通知は失われている
    Lost,
}

/// 配信役の時間まわり。既定値は本番用。テストでは短くする。
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// 死活確認（自分宛ての NOTIFY）を送る間隔
    pub ping_interval: Duration,
    /// この時間、返事が戻らなければ、LISTEN の接続は死んでいると判断する
    pub ping_timeout: Duration,
    /// 計測の定期ログの間隔
    pub log_interval: Duration,
    /// テスト用: ping の返事を無視する（LISTEN が黙って死んだ状態を再現する）。本番では常に false
    pub ignore_pongs: bool,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(15),
            ping_timeout: Duration::from_secs(5),
            log_interval: Duration::from_secs(300),
            ignore_pongs: false,
        }
    }
}

/// LISTEN の接続が生きているかの見張り。
///
/// 片側だけ切れた接続（半開きの TCP）は、読み書きのエラーにならず、通知が黙って来なくなる。
/// `try_recv` の「接続が切れた」検知では気づけないので、自分宛ての NOTIFY を定期的に送り、戻ってくるかを見る。
/// NOTIFY は同じ接続の中で送った順に届くので、番号 n が戻れば、それ以前の ping も届いている。
#[derive(Debug, Default)]
pub struct Watchdog {
    next: u64,
    /// まだ戻っていない、いちばん古い ping（番号, 送った時刻）
    pending: Option<(u64, Instant)>,
}

impl Watchdog {
    /// ping を送る。すでに戻っていない ping があれば、その時刻を保つ（送り直しで待ち時間が伸びない）
    pub fn start(&mut self, now: Instant) -> u64 {
        self.next += 1;
        self.pending.get_or_insert((self.next, now));
        self.next
    }

    /// 返事が戻った。往復の時間を返す（対応する ping が無ければ None）
    pub fn on_pong(&mut self, nonce: u64, now: Instant) -> Option<Duration> {
        match self.pending {
            Some((oldest, sent)) if nonce >= oldest => {
                self.pending = None;
                Some(now.saturating_duration_since(sent))
            }
            _ => None,
        }
    }

    /// 返事が timeout を超えて戻らないか
    pub fn is_dead(&self, now: Instant, timeout: Duration) -> bool {
        self.pending
            .is_some_and(|(_, sent)| now.saturating_duration_since(sent) > timeout)
    }
}

/// 配信役を起動する。落ちたら作り直す。
pub fn spawn(pool: PgPool, hub: Arc<Hub>, ai_agent_username: String) {
    spawn_with(pool, hub, ai_agent_username, Timing::default());
}

pub fn spawn_with(pool: PgPool, hub: Arc<Hub>, ai_agent_username: String, timing: Timing) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = run(&pool, &hub, &ai_agent_username, timing).await {
                tracing::error!(error = %e, "realtime dispatcher stopped; restarting");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

async fn run(
    pool: &PgPool,
    hub: &Arc<Hub>,
    ai_agent_username: &str,
    timing: Timing,
) -> anyhow::Result<()> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener
        .listen_all(["senn_sync", "senn_access", "senn_ping"])
        .await?;
    // 起動・再起動の間の変更は分からないので、つながっている端末には差分同期で埋めてもらう
    hub.resync_all("epoch_changed", &["ticket", "project", "comment"])
        .await;

    let watchdog = Arc::new(std::sync::Mutex::new(Watchdog::default()));
    let (tx, mut rx) = mpsc::channel::<Event>(4096);
    let reader_hub = hub.clone();
    let reader_watchdog = watchdog.clone();
    let ignore_pongs = timing.ignore_pongs;
    let reader = tokio::spawn(async move {
        loop {
            match listener.try_recv().await {
                Ok(Some(n)) => {
                    let ev = match n.channel() {
                        "senn_sync" => serde_json::from_str::<Notice>(n.payload())
                            .ok()
                            .map(Event::Sync),
                        "senn_access" => match n.payload().trim() {
                            "all" => Some(Event::Access(None)),
                            p => p.parse::<i32>().ok().map(|u| Event::Access(Some(u))),
                        },
                        "senn_ping" => {
                            // 返事は、配信の処理が忙しくても正しく測れるよう、この読み取りの場で記録する
                            if ignore_pongs {
                                continue;
                            }
                            if let Ok(nonce) = n.payload().trim().parse::<u64>() {
                                let rtt = reader_watchdog
                                    .lock()
                                    .ok()
                                    .and_then(|mut w| w.on_pong(nonce, Instant::now()));
                                if let Some(rtt) = rtt {
                                    reader_hub
                                        .stats
                                        .ping_roundtrip
                                        .record(rtt.as_millis() as u64);
                                }
                            }
                            continue;
                        }
                        _ => None,
                    };
                    match ev {
                        Some(ev) => {
                            if tx.send(ev).await.is_err() {
                                return;
                            }
                        }
                        None => tracing::warn!(
                            payload = n.payload(),
                            "realtime: unparsable notification"
                        ),
                    }
                }
                // 接続が切れた（次の呼び出しで自動的につなぎ直す）。その間の通知は失われている
                Ok(None) => {
                    reader_hub
                        .stats
                        .listener_lost
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if tx.send(Event::Lost).await.is_err() {
                        return;
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "realtime: listener error");
                    return;
                }
            }
        }
    });

    let mut recheck = tokio::time::interval(ACCESS_RECHECK);
    let mut sweep = tokio::time::interval(TOKEN_SWEEP);
    let mut ping = tokio::time::interval(timing.ping_interval);
    let mut stats_log = tokio::time::interval(timing.log_interval);
    ping.tick().await; // 最初の tick は即時なので捨てる
    stats_log.tick().await;
    let result: anyhow::Result<()> = loop {
        tokio::select! {
            first = rx.recv() => {
                let Some(first) = first else { break Err(anyhow::anyhow!("listener task ended")) };
                let started = Instant::now();
                let mut notices = Vec::new();
                let mut access_users = BTreeSet::new();
                let mut access_all = false;
                let mut lost = false;
                let mut absorb = |ev: Event| match ev {
                    Event::Sync(n) => notices.push(n),
                    Event::Access(Some(u)) => { access_users.insert(u); }
                    Event::Access(None) => access_all = true,
                    Event::Lost => lost = true,
                };
                absorb(first);
                let deadline = tokio::time::Instant::now() + COALESCE;
                while let Ok(Some(ev)) = tokio::time::timeout_at(deadline, rx.recv()).await {
                    absorb(ev);
                }
                if access_all {
                    access_users.extend(hub.connected_users().await);
                }
                for u in access_users {
                    if let Err(e) = recompute_access(pool, hub, u).await {
                        tracing::error!(error = %e, user_id = u, "realtime: access recompute failed");
                    }
                }
                if lost {
                    hub.resync_all("epoch_changed", &["ticket", "project", "comment"]).await;
                }
                if !notices.is_empty() {
                    if let Err(e) = dispatch(pool, hub, notices, ai_agent_username).await {
                        // 送れなかった分は差分同期で埋めてもらう
                        tracing::error!(error = %e, "realtime: dispatch failed");
                        hub.stats.dispatch_errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        hub.resync_all("slow_consumer", &["ticket", "project", "comment"]).await;
                    }
                    hub.stats.dispatch_latency.record(started.elapsed().as_millis() as u64);
                }
            }
            _ = ping.tick() => {
                let (dead, nonce) = {
                    let mut w = watchdog.lock().map_err(|_| anyhow::anyhow!("watchdog lock poisoned"))?;
                    let now = Instant::now();
                    let dead = w.is_dead(now, timing.ping_timeout);
                    (dead, if dead { 0 } else { w.start(now) })
                };
                if dead {
                    // 返事が戻らない = LISTEN の接続が黙って死んでいる。その間の通知は失われているので、全部屋に再同期を求め、接続を作り直す
                    tracing::error!(timeout_ms = timing.ping_timeout.as_millis() as u64, "realtime: LISTEN connection unresponsive; reconnecting");
                    hub.stats.watchdog_timeouts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    hub.resync_all("epoch_changed", &["ticket", "project", "comment"]).await;
                    break Err(anyhow::anyhow!("LISTEN connection unresponsive"));
                }
                if let Err(e) = sqlx::query("SELECT pg_notify('senn_ping', $1)").bind(nonce.to_string()).execute(pool).await {
                    tracing::warn!(error = %e, "realtime: ping send failed");
                }
            }
            _ = stats_log.tick() => {
                let snap = hub.snapshot().await;
                tracing::info!(target: "realtime_stats", stats = %serde_json::to_string(&snap).unwrap_or_default(), "realtime stats");
            }
            _ = recheck.tick() => {
                let pruned = hub.prune_idle_rooms(IDLE_ROOM_TTL).await;
                if pruned > 0 {
                    tracing::debug!(pruned, "realtime: pruned idle rooms");
                }
                for u in hub.connected_users().await {
                    if let Err(e) = recompute_access(pool, hub, u).await {
                        tracing::error!(error = %e, user_id = u, "realtime: access recheck failed");
                    }
                }
            }
            _ = sweep.tick() => {
                if let Err(e) = sqlx::query("DELETE FROM realtime_connect_tokens WHERE expires_at < NOW()").execute(pool).await {
                    tracing::warn!(error = %e, "realtime: token sweep failed");
                }
            }
        }
    };
    reader.abort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(
        e: &str,
        a: &str,
        id: i64,
        v: i64,
        t: Option<i32>,
        p: Option<i32>,
        ot: Option<i32>,
        opj: Option<i32>,
    ) -> Notice {
        Notice {
            e: e.into(),
            a: a.into(),
            id,
            v,
            t,
            p,
            ot,
            opj,
            u: None,
            teams: vec![],
        }
    }

    #[test]
    fn merge_keeps_last_per_row_and_projects_first() {
        let m = merge(vec![
            n("ticket", "insert", 1, 1, Some(3), Some(12), None, None),
            n("ticket", "update", 1, 2, Some(3), Some(12), None, None),
            n("project", "update", 12, 5, None, None, None, None),
            n("ticket", "update", 2, 7, Some(3), None, None, None),
        ]);
        assert_eq!(
            m.iter()
                .map(|x| (x.entity.as_str(), x.id))
                .collect::<Vec<_>>(),
            vec![("project", 12), ("ticket", 1), ("ticket", 2)]
        );
        let t1 = m.iter().find(|x| x.id == 1).unwrap();
        assert_eq!((t1.last_action.as_str(), t1.v), ("update", 2));
    }

    #[test]
    fn merge_remembers_old_places_across_a_move_and_a_later_update() {
        // T1 → T2 に移動した直後に、同じ窓の中でもう一度更新（後の通知には ot が無い）
        let m = merge(vec![
            n("ticket", "update", 1, 2, Some(2), None, Some(1), None),
            n("ticket", "update", 1, 3, Some(2), None, None, None),
        ]);
        assert!(
            m[0].places.contains(&(Some(1), None)),
            "移動前の場所を忘れた: {:?}",
            m[0].places
        );
        assert!(m[0].places.contains(&(Some(2), None)));
    }

    #[test]
    fn delete_goes_to_every_room_the_row_was_ever_in() {
        let m = merge(vec![
            n("ticket", "update", 1, 2, Some(2), None, Some(1), None),
            n("ticket", "delete", 1, 3, Some(2), None, None, None),
        ]);
        let plan = plan(&m, &HashMap::new(), &HashMap::new());
        let rooms: Vec<&str> = plan.keys().map(|r| r.0.as_str()).collect();
        assert_eq!(rooms, vec!["all", "t:1", "t:2"]);
        assert!(plan
            .values()
            .all(|c| matches!(c[0], Change::Delete { v: 3, .. })));
    }

    #[test]
    fn project_changes_are_signals_in_the_global_room() {
        let plan = plan(
            &merge(vec![n("project", "update", 12, 5, None, None, None, None)]),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(
            plan.keys().map(|r| r.0.as_str()).collect::<Vec<_>>(),
            vec!["g"]
        );
        assert!(matches!(
            plan[&global_room()][0],
            Change::Stale { id: 12, v: 5, .. }
        ));
        let del = plan_of_delete_project();
        assert!(matches!(
            del[&global_room()][0],
            Change::Delete { id: 12, .. }
        ));
    }

    fn plan_of_delete_project() -> BTreeMap<RoomId, Vec<Change>> {
        plan(
            &merge(vec![n("project", "delete", 12, 6, None, None, None, None)]),
            &HashMap::new(),
            &HashMap::new(),
        )
    }

    #[test]
    fn ticket_vanished_before_refetch_is_skipped() {
        let m = merge(vec![n("ticket", "update", 1, 2, Some(3), None, None, None)]);
        assert!(plan(&m, &HashMap::new(), &HashMap::new()).is_empty());
    }

    fn sig(
        e: &str,
        id: i64,
        t: Option<i32>,
        p: Option<i32>,
        u: Option<i32>,
        teams: &[i32],
    ) -> Notice {
        Notice {
            e: e.into(),
            a: "update".into(),
            id,
            v: 0,
            t,
            p,
            ot: None,
            opj: None,
            u,
            teams: teams.to_vec(),
        }
    }

    fn rooms_of(plan: &BTreeMap<RoomId, Vec<Change>>) -> Vec<&str> {
        plan.keys().map(|r| r.0.as_str()).collect()
    }

    #[test]
    fn attachment_and_reaction_signals_go_to_the_ticket_rooms() {
        let plan = plan(
            &merge(vec![
                sig("attachment", 7, Some(3), Some(12), None, &[]),
                sig("reaction", 55, Some(3), Some(12), None, &[]),
            ]),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(rooms_of(&plan), vec!["all", "p:3:12", "t:3"]);
        let changes = &plan[&RoomId::team(3)];
        assert!(
            changes
                .iter()
                .all(|c| matches!(c, Change::Stale { v: 0, .. })),
            "実データは載せない: {changes:?}"
        );
        assert!(changes
            .iter()
            .any(|c| matches!(c, Change::Stale { entity, id: 7, .. } if entity == "attachment")));
        // リアクションの id は「チケットの id」（そのチケットのリアクションを取り直す）
        assert!(changes
            .iter()
            .any(|c| matches!(c, Change::Stale { entity, id: 55, .. } if entity == "reaction")));
    }

    #[test]
    fn notifications_go_only_to_the_owner_and_bulk_changes_collapse_to_one_signal() {
        let notices: Vec<Notice> = (1..=40)
            .map(|i| sig("notification", i, None, None, Some(5), &[]))
            .collect();
        let plan = plan(&merge(notices), &HashMap::new(), &HashMap::new());
        assert_eq!(rooms_of(&plan), vec!["u:5"], "本人の部屋だけ");
        let changes = &plan[&RoomId::user(5)];
        assert_eq!(
            changes.len(),
            1,
            "40件の変更が1つの合図にまとまる: {changes:?}"
        );
        assert!(
            matches!(&changes[0], Change::Stale { entity, id: 0, .. } if entity == "notification")
        );
    }

    #[test]
    fn different_users_notifications_do_not_leak_between_rooms() {
        let plan = plan(
            &merge(vec![
                sig("notification", 1, None, None, Some(5), &[]),
                sig("notification", 2, None, None, Some(6), &[]),
            ]),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(rooms_of(&plan), vec!["u:5", "u:6"]);
        assert_eq!(plan[&RoomId::user(5)].len(), 1);
    }

    #[test]
    fn cycle_and_wiki_signals_follow_the_project_and_global_wiki_goes_to_everyone() {
        let plan = plan(
            &merge(vec![
                sig("cycle", 3, Some(3), Some(12), None, &[]),
                sig("wiki", 9, None, Some(12), None, &[5]),
            ]),
            &HashMap::new(),
            &HashMap::new(),
        );
        // サイクルはチーム3、Wiki はプロジェクト12の所属チーム5
        assert_eq!(
            rooms_of(&plan),
            vec!["all", "p:3:12", "p:5:12", "t:3", "t:5"]
        );
        assert!(plan[&RoomId::team(3)]
            .iter()
            .all(|c| matches!(c, Change::Stale { entity, .. } if entity == "cycle")));
        assert!(plan[&RoomId::team(5)]
            .iter()
            .all(|c| matches!(c, Change::Stale { entity, .. } if entity == "wiki")));
        // プロジェクトなしの Wiki は全員
        let global = super::plan(
            &merge(vec![sig("wiki", 1, None, None, None, &[])]),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(rooms_of(&global), vec!["g"]);
        // チーム専用の Wiki は、そのチームだけ（全員宛にしない）
        let team_only = super::plan(
            &merge(vec![sig("wiki", 2, Some(4), None, None, &[])]),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(rooms_of(&team_only), vec!["all", "t:4"]);
    }

    #[test]
    fn signals_come_after_real_data_in_the_same_packet() {
        let m = merge(vec![
            sig("attachment", 7, Some(3), None, None, &[]),
            n("ticket", "update", 1, 2, Some(3), None, None, None),
        ]);
        assert_eq!(
            m.iter().map(|x| x.entity.as_str()).collect::<Vec<_>>(),
            vec!["ticket", "attachment"]
        );
    }

    #[test]
    fn a_signal_without_a_destination_is_dropped() {
        // 宛先の無い通知（u なし）は、どの部屋にも流さない
        assert!(plan(
            &merge(vec![sig("notification", 1, None, None, None, &[])]),
            &HashMap::new(),
            &HashMap::new()
        )
        .is_empty());
    }

    #[test]
    fn watchdog_reports_round_trip_and_clears_the_pending_ping() {
        let mut w = Watchdog::default();
        let t0 = Instant::now();
        let n = w.start(t0);
        assert!(!w.is_dead(t0 + Duration::from_secs(4), Duration::from_secs(5)));
        let rtt = w
            .on_pong(n, t0 + Duration::from_millis(30))
            .expect("返事が戻った");
        assert_eq!(rtt, Duration::from_millis(30));
        // 戻ったあとは、いくら経っても死んでいない
        assert!(!w.is_dead(t0 + Duration::from_secs(600), Duration::from_secs(5)));
    }

    #[test]
    fn watchdog_declares_dead_only_after_the_timeout_with_no_reply() {
        let mut w = Watchdog::default();
        let t0 = Instant::now();
        w.start(t0);
        assert!(
            !w.is_dead(t0 + Duration::from_secs(5), Duration::from_secs(5)),
            "ちょうど timeout はまだ"
        );
        assert!(w.is_dead(t0 + Duration::from_millis(5001), Duration::from_secs(5)));
    }

    #[test]
    fn a_resent_ping_does_not_extend_the_wait() {
        let mut w = Watchdog::default();
        let t0 = Instant::now();
        w.start(t0);
        // 返事が無いまま次の ping を送っても、最初の ping の時刻から数える
        w.start(t0 + Duration::from_secs(3));
        assert!(w.is_dead(t0 + Duration::from_secs(6), Duration::from_secs(5)));
    }

    #[test]
    fn a_later_pong_also_clears_earlier_pings_and_stale_pongs_are_ignored() {
        let mut w = Watchdog::default();
        let t0 = Instant::now();
        let first = w.start(t0);
        let second = w.start(t0 + Duration::from_secs(1));
        assert!(second > first);
        // 通知は送った順に届くので、2番目が戻れば1番目も届いている
        assert!(w.on_pong(second, t0 + Duration::from_secs(2)).is_some());
        assert!(!w.is_dead(t0 + Duration::from_secs(60), Duration::from_secs(5)));
        // 何も待っていないときの返事は無視する
        assert!(w.on_pong(second, t0 + Duration::from_secs(3)).is_none());
    }
}
