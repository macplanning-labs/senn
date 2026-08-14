/// presentation/filters.rs — Askamaテンプレート用カスタムフィルタ
///
/// askama 0.13には日時フォーマット用の組み込み`date`フィルタが無いため、
/// テンプレート側の `{{ ... |date }}` 呼び出しに対応する実装をここに置く。
use chrono::{DateTime, Utc};

pub fn date(value: &DateTime<Utc>) -> askama::Result<String> {
    Ok(value.format("%Y-%m-%d %H:%M").to_string())
}

/// Django/Jinja2の`default`フィルタ相当（askama 0.13に組み込みが無いため独自実装）。
/// `Option<String>`フィールドに対して `{{ x|default("-") }}` の形で使う。
pub fn default(value: &Option<String>, fallback: &str) -> askama::Result<String> {
    Ok(match value {
        Some(v) => v.clone(),
        None => fallback.to_string(),
    })
}
