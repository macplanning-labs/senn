//! stats.rs — リアルタイム配信の計測（接続数・配信数・遅れ・メモリの目安）
//!
//! 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §17
//! 管理者用 API（GET /api/v1/system-admin/realtime/stats/）と、定期ログ（5分ごと）で見る。
//! 数えるだけで、配信の経路には影響しない（アトミックな加算のみ）。
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use serde::Serialize;

/// 配信役の処理時間（通知を受けてから、部屋への配信が終わるまで）の集計
#[derive(Default)]
pub struct LatencyStat {
    count: AtomicU64,
    total_ms: AtomicU64,
    max_ms: AtomicU64,
    last_ms: AtomicU64,
}

impl LatencyStat {
    pub fn record(&self, ms: u64) {
        self.count.fetch_add(1, Relaxed);
        self.total_ms.fetch_add(ms, Relaxed);
        self.last_ms.store(ms, Relaxed);
        self.max_ms.fetch_max(ms, Relaxed);
    }

    fn snapshot(&self) -> LatencySnapshot {
        let count = self.count.load(Relaxed);
        LatencySnapshot {
            count,
            avg_ms: if count == 0 {
                0.0
            } else {
                self.total_ms.load(Relaxed) as f64 / count as f64
            },
            max_ms: self.max_ms.load(Relaxed),
            last_ms: self.last_ms.load(Relaxed),
        }
    }
}

#[derive(Default)]
pub struct Stats {
    /// 部屋に配ったパケット数（部屋ごとに1回。購読者の数は掛けない）
    pub published_packets: AtomicU64,
    /// 配ったパケットの本文の合計バイト数（部屋ごとに1回）
    pub published_bytes: AtomicU64,
    /// 購読者への送信の合計（パケット × 購読者数）
    pub deliveries: AtomicU64,
    /// 送信待ちがあふれて切った接続の数
    pub slow_drops: AtomicU64,
    /// 同時接続の上限を超えて、古い接続を切った数
    pub cap_evictions: AtomicU64,
    /// 全部屋への再同期（reason 別）
    pub resync_epoch_changed: AtomicU64,
    pub resync_bulk_change: AtomicU64,
    pub resync_slow_consumer: AtomicU64,
    /// 再接続のとき、リングから再送できず resync にした部屋の数
    pub resume_resyncs: AtomicU64,
    /// 再接続のとき、リングから再送できた部屋の数
    pub resume_replays: AtomicU64,
    /// 配信役の処理の失敗
    pub dispatch_errors: AtomicU64,
    /// LISTEN の接続が切れた回数（切断を検知したもの）
    pub listener_lost: AtomicU64,
    /// 死活確認（ping）に返事が無く、接続を作り直した回数
    pub watchdog_timeouts: AtomicU64,
    /// 通知を受けてから配信が終わるまで
    pub dispatch_latency: LatencyStat,
    /// 死活確認の往復（NOTIFY が LISTEN に戻ってくるまで）
    pub ping_roundtrip: LatencyStat,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LatencySnapshot {
    pub count: u64,
    #[serde(rename = "avgMs")]
    pub avg_ms: f64,
    #[serde(rename = "maxMs")]
    pub max_ms: u64,
    #[serde(rename = "lastMs")]
    pub last_ms: u64,
}

/// 管理者用 API と定期ログの中身
#[derive(Debug, Serialize)]
pub struct StatsSnapshot {
    pub epoch: String,
    /// 現在の状態（Hub が持つもの）
    pub connections: usize,
    pub users: usize,
    pub rooms: usize,
    /// リング（再送用）に残っているパケット数と本文の合計バイト数。メモリの目安
    #[serde(rename = "ringPackets")]
    pub ring_packets: usize,
    #[serde(rename = "ringBytes")]
    pub ring_bytes: usize,
    /// もっとも大きい部屋の上位（部屋 ID, 購読者数, リングのバイト数）
    #[serde(rename = "topRooms")]
    pub top_rooms: Vec<RoomSnapshot>,
    /// 起動してからの累計
    pub totals: Totals,
    #[serde(rename = "dispatchLatency")]
    pub dispatch_latency: LatencySnapshot,
    #[serde(rename = "pingRoundtrip")]
    pub ping_roundtrip: LatencySnapshot,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RoomSnapshot {
    pub room: String,
    pub members: usize,
    #[serde(rename = "ringBytes")]
    pub ring_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct Totals {
    #[serde(rename = "publishedPackets")]
    pub published_packets: u64,
    #[serde(rename = "publishedBytes")]
    pub published_bytes: u64,
    pub deliveries: u64,
    #[serde(rename = "slowDrops")]
    pub slow_drops: u64,
    #[serde(rename = "capEvictions")]
    pub cap_evictions: u64,
    #[serde(rename = "resyncEpochChanged")]
    pub resync_epoch_changed: u64,
    #[serde(rename = "resyncBulkChange")]
    pub resync_bulk_change: u64,
    #[serde(rename = "resyncSlowConsumer")]
    pub resync_slow_consumer: u64,
    #[serde(rename = "resumeResyncs")]
    pub resume_resyncs: u64,
    #[serde(rename = "resumeReplays")]
    pub resume_replays: u64,
    #[serde(rename = "dispatchErrors")]
    pub dispatch_errors: u64,
    #[serde(rename = "listenerLost")]
    pub listener_lost: u64,
    #[serde(rename = "watchdogTimeouts")]
    pub watchdog_timeouts: u64,
}

impl Stats {
    pub fn totals(&self) -> Totals {
        Totals {
            published_packets: self.published_packets.load(Relaxed),
            published_bytes: self.published_bytes.load(Relaxed),
            deliveries: self.deliveries.load(Relaxed),
            slow_drops: self.slow_drops.load(Relaxed),
            cap_evictions: self.cap_evictions.load(Relaxed),
            resync_epoch_changed: self.resync_epoch_changed.load(Relaxed),
            resync_bulk_change: self.resync_bulk_change.load(Relaxed),
            resync_slow_consumer: self.resync_slow_consumer.load(Relaxed),
            resume_resyncs: self.resume_resyncs.load(Relaxed),
            resume_replays: self.resume_replays.load(Relaxed),
            dispatch_errors: self.dispatch_errors.load(Relaxed),
            listener_lost: self.listener_lost.load(Relaxed),
            watchdog_timeouts: self.watchdog_timeouts.load(Relaxed),
        }
    }

    pub fn dispatch_latency(&self) -> LatencySnapshot {
        self.dispatch_latency.snapshot()
    }

    pub fn ping_roundtrip(&self) -> LatencySnapshot {
        self.ping_roundtrip.snapshot()
    }

    /// reason 別の全部屋 resync の数え上げ
    pub fn count_resync(&self, reason: &str) {
        let c = match reason {
            "epoch_changed" => &self.resync_epoch_changed,
            "bulk_change" => &self.resync_bulk_change,
            _ => &self.resync_slow_consumer,
        };
        c.fetch_add(1, Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_tracks_count_avg_max_last() {
        let l = LatencyStat::default();
        assert_eq!(
            l.snapshot(),
            LatencySnapshot {
                count: 0,
                avg_ms: 0.0,
                max_ms: 0,
                last_ms: 0
            }
        );
        l.record(10);
        l.record(50);
        l.record(30);
        assert_eq!(
            l.snapshot(),
            LatencySnapshot {
                count: 3,
                avg_ms: 30.0,
                max_ms: 50,
                last_ms: 30
            }
        );
    }

    #[test]
    fn resync_reasons_are_counted_separately() {
        let s = Stats::default();
        s.count_resync("epoch_changed");
        s.count_resync("epoch_changed");
        s.count_resync("bulk_change");
        s.count_resync("slow_consumer");
        let t = s.totals();
        assert_eq!(
            (
                t.resync_epoch_changed,
                t.resync_bulk_change,
                t.resync_slow_consumer
            ),
            (2, 1, 1)
        );
    }
}
