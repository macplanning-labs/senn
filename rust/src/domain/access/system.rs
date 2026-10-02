//! SystemContext — 閲覧者のいない処理(定期処理・通知の送信・マイグレーション)が、全体のデータを読むための証明
//!
//! テナントのデータを読むリポジトリ関数は、`Scope`(閲覧者の範囲)か、この `SystemContext` のどちらかを必須の引数にする。
//! ハンドラ(利用者の操作)からは作れない: 構築子は、許された場所(定期処理・通知の送信・テスト)にだけ公開する。

/// 閲覧者のいない処理であることの証明(中身は無い。型だけが意味を持つ)
#[derive(Debug)]
pub struct SystemContext {
    _private: (),
}

impl SystemContext {
    /// 定期処理・通知の送信・テストからだけ呼ぶ。**ハンドラから呼ばない**(レビューで確認し、
    /// `routes.rs` のルート検査テストが、ハンドラのファイルでの使用を検出する)。
    pub(crate) fn for_background_job(reason: &'static str) -> Self {
        tracing::debug!(reason, "SystemContext created");
        SystemContext { _private: () }
    }
}
