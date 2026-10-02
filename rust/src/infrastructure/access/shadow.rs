//! 試運転(shadow): 新しい判定と今の判定を両方計算し、違いを記録する(詳細設計書 §9.3)
//!
//! リソースの種類ごとに、環境変数 `ACCESS_ENFORCE_<種類>` で切り替える。
//! - `off`    : 今の判定だけ(記録もしない)
//! - `shadow` : 応答は今の判定のまま、違いを `access_shadow_diff` に記録する(既定)
//! - `on`     : 新しい判定だけを使う
//!
//! 記録には本文を入れない(種類・方向・ユーザー・リソースの ID・ルートだけ)。
//! 記録の失敗で、利用者の操作を失敗させない(ログに残して続ける)。

use std::collections::BTreeSet;

use sqlx::PgPool;

/// リソースの種類(切り替えの単位)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Ticket,
    Project,
    Team,
    Wiki,
    Cycle,
    Master,
    User,
    Search,
    Sync,
    Realtime,
    Key,
}

impl Resource {
    pub fn as_str(self) -> &'static str {
        match self {
            Resource::Ticket => "ticket",
            Resource::Project => "project",
            Resource::Team => "team",
            Resource::Wiki => "wiki",
            Resource::Cycle => "cycle",
            Resource::Master => "master",
            Resource::User => "user",
            Resource::Search => "search",
            Resource::Sync => "sync",
            Resource::Realtime => "realtime",
            Resource::Key => "key",
        }
    }

    fn env_name(self) -> String {
        format!("ACCESS_ENFORCE_{}", self.as_str().to_ascii_uppercase())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Shadow,
    On,
}

impl Mode {
    /// 設定値の解釈。未設定・不明な値は `Shadow`(見え方を変えない側)
    pub fn parse(v: Option<&str>) -> Mode {
        match v.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            Some("off") => Mode::Off,
            Some("on") => Mode::On,
            _ => Mode::Shadow,
        }
    }
}

/// 今の切り替えの状態。環境変数を毎回読む(読む費用は小さい。値の変更は、コンテナの再起動で反映する)
pub fn mode(resource: Resource) -> Mode {
    Mode::parse(std::env::var(resource.env_name()).ok().as_deref())
}

/// 違いの方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// 今は見えるが、新しい判定では見えない
    NewlyHidden,
    /// 今は見えないが、新しい判定では見える(漏れにつながる側。特に確認する)
    NewlyVisible,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Direction::NewlyHidden => "newly_hidden",
            Direction::NewlyVisible => "newly_visible",
        }
    }
}

/// 一覧の、新旧の ID の集合の違い(記録する物だけを返す。純粋な関数)
pub fn diff_ids(current: &[i64], new: &[i64]) -> Vec<(Direction, i64)> {
    let cur: BTreeSet<i64> = current.iter().copied().collect();
    let nw: BTreeSet<i64> = new.iter().copied().collect();
    cur.difference(&nw)
        .map(|&id| (Direction::NewlyHidden, id))
        .chain(nw.difference(&cur).map(|&id| (Direction::NewlyVisible, id)))
        .collect()
}

/// 1 回のリクエストで記録する件数の上限(大きな一覧で、記録が利用者の操作を遅くしないように)
const MAX_RECORDS_PER_CALL: usize = 200;

/// 違いを記録する(裏で実行し、利用者の応答を待たせない)
pub fn record(
    pool: &PgPool,
    resource: Resource,
    user_id: Option<i32>,
    route: &'static str,
    diffs: Vec<(Direction, i64)>,
) {
    if diffs.is_empty() {
        return;
    }
    let pool = pool.clone();
    let user_id = user_id.unwrap_or(0) as i64;
    tokio::spawn(async move {
        for (dir, id) in diffs.into_iter().take(MAX_RECORDS_PER_CALL) {
            if let Err(e) = sqlx::query(
                "INSERT INTO access_shadow_diff (resource, direction, user_id, resource_id, route)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(resource.as_str())
            .bind(dir.as_str())
            .bind(user_id)
            .bind(id)
            .bind(route)
            .execute(&pool)
            .await
            {
                tracing::warn!("[認可/試運転] 差分の記録に失敗(操作は続行): {:?}", e);
                return;
            }
        }
    });
}

/// 個別の判定(見えるか)の、新旧の比較と、応答に使う結果
///
/// 戻り値: 応答に使う「見えるか」。`Shadow` は今の判定、`On` は新しい判定、`Off` は今の判定(記録しない)。
pub fn decide_single(
    pool: &PgPool,
    resource: Resource,
    user_id: Option<i32>,
    route: &'static str,
    resource_id: i64,
    current_visible: bool,
    new_visible: bool,
) -> bool {
    match mode(resource) {
        Mode::Off => current_visible,
        Mode::On => new_visible,
        Mode::Shadow => {
            if current_visible != new_visible {
                let dir = if current_visible {
                    Direction::NewlyHidden
                } else {
                    Direction::NewlyVisible
                };
                record(pool, resource, user_id, route, vec![(dir, resource_id)]);
            }
            current_visible
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_defaults_to_shadow() {
        assert_eq!(Mode::parse(None), Mode::Shadow);
        assert_eq!(Mode::parse(Some("")), Mode::Shadow);
        assert_eq!(Mode::parse(Some("typo")), Mode::Shadow);
        assert_eq!(Mode::parse(Some("OFF")), Mode::Off);
        assert_eq!(Mode::parse(Some(" on ")), Mode::On);
    }

    #[test]
    fn diff_reports_both_directions() {
        let d = diff_ids(&[1, 2, 3], &[2, 3, 4]);
        assert_eq!(
            d,
            vec![(Direction::NewlyHidden, 1), (Direction::NewlyVisible, 4)]
        );
        assert!(diff_ids(&[1, 2], &[2, 1]).is_empty());
    }

    #[test]
    fn env_names() {
        assert_eq!(Resource::Ticket.env_name(), "ACCESS_ENFORCE_TICKET");
        assert_eq!(Resource::Realtime.env_name(), "ACCESS_ENFORCE_REALTIME");
    }

    #[tokio::test]
    async fn shadow_records_and_keeps_current_answer() {
        let Some(pool) = crate::test_support::test_pool().await else {
            return;
        };
        // 環境変数を触らない(既定 = Shadow)。今は見える・新しくは見えない → 今の答え(true)を返し、記録する
        let id = 900_000_000 + (std::process::id() as i64 % 1000);
        let got = decide_single(&pool, Resource::Cycle, Some(1), "test", id, true, false);
        assert!(got, "試運転では、今の判定の答えを返す");
        // 記録は裏で行われるので、少し待って確認する
        let mut found = 0i64;
        for _ in 0..50 {
            found = sqlx::query_scalar("SELECT count(*) FROM access_shadow_diff WHERE resource = 'cycle' AND resource_id = $1 AND direction = 'newly_hidden'")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
            if found > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(found, 1);
    }
}
