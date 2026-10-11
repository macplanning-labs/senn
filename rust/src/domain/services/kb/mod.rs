//! KB知識エンジンモジュール
//!
//! Scryer Prolog ベースの知識推論エンジン。
//! クローズ判定・プロンプト生成・類似障害検索に利用される。

// 第2・3弾(プロンプト生成・類似障害検索)で使う口を含むため、未使用の警告は出さない
#![allow(dead_code)]

pub mod atom;
pub mod close_check;
pub mod config;
pub mod detect;
pub mod facts;
pub mod messages;
pub mod overlap;
pub mod parse;
pub mod pattern;
pub mod pattern_check;
pub mod runner;
pub mod sections;
pub mod similar;
pub mod summary;
pub mod summary_check;

use thiserror::Error;

/// KB エンジンの実行結果型
#[derive(Debug, Clone, PartialEq)]
pub struct CloseCheckResult {
    pub can_close: bool,
    pub blockers: Vec<Blocker>,
}

/// クローズを妨げるブロッカー
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Blocker {
    pub code: String,
    pub arg: String,
}

/// KB エンジンのエラー型
#[derive(Debug, Clone, PartialEq, Error)]
pub enum KbError {
    /// KB エンジンが無効(SENN_KB_ENGINE=off)
    #[error("KB engine is disabled")]
    Disabled,

    /// Prolog 実行ビジーだが後で再試行可能
    #[error("KB engine is busy")]
    Busy,

    /// Prolog 実行タイムアウト
    #[error("KB engine timeout")]
    Timeout,

    /// Prolog プロセス失敗(stderr 出力・非0終了等)
    #[error("KB process failed: {0}")]
    ProcessFailed(String),

    /// Prolog 出力が解釈不能(形式不正・予期しない内容)
    #[error("KB output parsing failed: {0}")]
    BadOutput(String),

    /// 入力(事実)が大きすぎる(1MB超過)
    #[error("KB input too large")]
    TooLarge,

    /// 入力が無効(述語名が不正等)
    #[error("KB invalid input: {0}")]
    InvalidInput(String),

    /// データベースエラー
    #[error("KB database error: {0}")]
    Db(String),
}
