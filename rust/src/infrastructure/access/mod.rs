//! infrastructure/access — アクセス制御の、DB に触る部分
//!
//! - `viewer_repo` : 閲覧者(Viewer)の読み込み
//! - `facts_repo`  : リソースの所属情報を小さな問い合わせで作る
//! - `scope_sql`   : 見える範囲を WHERE 句にする唯一の場所
//!
//! 規則そのものは `domain::access` に置く。ここに規則を書かない。

// フェーズ B(土台)の時点では、判定・絞り込みはテストと抽出器からだけ使う。ハンドラ・リポジトリから使い始める
// フェーズ C で、この抑止を外す(タスクリスト C-1)。
#![allow(dead_code)]

pub mod facts_repo;
pub mod recipients;
pub mod scope_sql;
pub mod shadow;
pub mod viewer_repo;

#[cfg(test)]
mod parity_tests;
