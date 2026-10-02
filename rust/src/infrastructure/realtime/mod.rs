//! リアルタイム同期（Delta Push）
//!
//! 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md
//! - rooms: 行・利用者と部屋の対応
//! - hub: 部屋・接続・seq・再送用リング
//! - dispatcher: DB の変更通知 → 部屋ごとの差分パケット
pub mod dispatcher;
pub mod hub;
pub mod rooms;
pub mod stats;

#[cfg(test)]
mod db_tests;
