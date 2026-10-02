//! 移行の確認の一覧(アクセス制御の再設計 G-6。詳細設計書 §12.1)
//!
//! 新しい規則を有効にする前(フェーズ H-1)に、システム管理者が判断するための材料を 1 回で返す。
//! 本番には SSH できないため、管理コマンドではなく、システム管理者だけが呼べる API にした。
//!
//! - GET /api/v1/system-admin/access-migration-report/
//!
//! 一覧は判断の材料だけ(チケットの本文などの中身は返さない)。

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

use crate::domain::access::Viewer;
use crate::presentation::state::AppState;

/// 社内のメールのドメイン(`SENN_INTERNAL_EMAIL_DOMAINS=example.com,example.co.jp`)。未設定なら空
fn internal_domains() -> Vec<String> {
    std::env::var("SENN_INTERNAL_EMAIL_DOMAINS")
        .unwrap_or_default()
        .split(',')
        .map(|d| d.trim().to_ascii_lowercase())
        .filter(|d| !d.is_empty())
        .collect()
}

/// 固定の SQL(`&'static str` = コードに書いた文字列だけ。外からの入力は渡せない)の結果を JSON の行にする
async fn rows_json(pool: &PgPool, sql: &'static str) -> sqlx::Result<Vec<Value>> {
    let mut qb =
        sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT row_to_json(x)::jsonb AS j FROM (");
    qb.push(sql).push(") x");
    let rows = qb.build().fetch_all(pool).await?;
    Ok(rows.iter().map(|r| r.get::<Value, _>("j")).collect())
}

async fn build(pool: &PgPool) -> sqlx::Result<Value> {
    // 1. 全チーム(機密のチームを Private にする判断)
    let teams = rows_json(
        pool,
        "SELECT m.id, m.name, m.slug, m.visibility, m.settings_policy AS \"settingsPolicy\",
                (m.archived_at IS NOT NULL) AS archived,
                (SELECT COUNT(DISTINCT tm.user_id) FROM t_team_membership tm JOIN accounts_user u ON u.id = tm.user_id
                  WHERE tm.team_id = m.id AND tm.scoped_project_id IS NULL AND u.is_active AND NOT u.is_guest) AS \"fullMemberCount\",
                (SELECT COUNT(DISTINCT tm.user_id) FROM t_team_membership tm JOIN accounts_user u ON u.id = tm.user_id
                  WHERE tm.team_id = m.id AND u.is_active AND (u.is_guest OR tm.scoped_project_id IS NOT NULL)) AS \"guestCount\",
                (SELECT COUNT(*) FROM t_team_membership tm WHERE tm.team_id = m.id AND tm.role = 'admin' AND tm.scoped_project_id IS NULL) AS \"ownerCount\",
                (SELECT COUNT(*) FROM tickets_project_teams pt WHERE pt.team_id = m.id) AS \"projectCount\",
                (SELECT COUNT(*) FROM tickets_ticket t WHERE t.team_id = m.id) AS \"ticketCount\"
         FROM m_team m ORDER BY m.name",
    )
    .await?;
    // 2. is_staff のユーザー(is_system_admin の確認)
    let staff = rows_json(
        pool,
        "SELECT id, username, display_name AS \"displayName\", email, is_active AS \"isActive\",
                is_system_admin AS \"isSystemAdmin\"
         FROM accounts_user WHERE is_staff OR is_system_admin ORDER BY username",
    )
    .await?;
    // 3. プロジェクト単位の所属だけを持つ人(Guest にする候補)
    let project_only = rows_json(
        pool,
        "SELECT u.id, u.username, u.display_name AS \"displayName\", u.email, u.is_guest AS \"isGuest\",
                (SELECT json_agg(json_build_object('teamId', tm.team_id, 'projectId', tm.scoped_project_id, 'endDate', tm.end_date))
                   FROM t_team_membership tm WHERE tm.user_id = u.id) AS memberships
         FROM accounts_user u
         WHERE u.is_active
           AND EXISTS (SELECT 1 FROM t_team_membership tm WHERE tm.user_id = u.id AND tm.scoped_project_id IS NOT NULL)
           AND NOT EXISTS (SELECT 1 FROM t_team_membership tm WHERE tm.user_id = u.id AND tm.scoped_project_id IS NULL)
         ORDER BY u.username",
    )
    .await?;
    // 4. メールのドメイン別の人数と、社内のドメインでない人(社内のドメインが設定されているとき)
    let domains = rows_json(
        pool,
        "SELECT lower(split_part(email, '@', 2)) AS domain, COUNT(*) AS \"userCount\"
         FROM accounts_user WHERE is_active AND email LIKE '%@%'
         GROUP BY 1 ORDER BY 2 DESC",
    )
    .await?;
    let internal = internal_domains();
    let external_users = if internal.is_empty() {
        Vec::new()
    } else {
        let rows = sqlx::query(
            "SELECT row_to_json(x)::jsonb AS j FROM (
               SELECT id, username, display_name AS \"displayName\", email, is_guest AS \"isGuest\"
               FROM accounts_user
               WHERE is_active AND NOT (lower(split_part(email, '@', 2)) = ANY($1))
               ORDER BY username) x",
        )
        .bind(&internal)
        .fetch_all(pool)
        .await?;
        rows.iter().map(|r| r.get::<Value, _>("j")).collect()
    };
    // 5. チームの無いチケット(所属させるチームの判断。本文は返さない)
    let teamless = rows_json(
        pool,
        "SELECT t.id, t.ticket_key AS \"ticketKey\", t.status, p.prefix AS \"projectPrefix\",
                a.username AS author
         FROM tickets_ticket t
         LEFT JOIN tickets_project p ON p.id = t.project_id
         LEFT JOIN accounts_user a ON a.id = t.author_id
         WHERE t.team_id IS NULL ORDER BY t.id LIMIT 500",
    )
    .await?;
    // 6. プロジェクトのオーナーだが、参加チームの Owner ではない人(自動昇格しない。判断の材料)
    let owners_not_team_owners = rows_json(
        pool,
        "SELECT p.id AS \"projectId\", p.prefix, p.name, u.username AS owner,
                (SELECT json_agg(m.name ORDER BY m.name) FROM tickets_project_teams pt JOIN m_team m ON m.id = pt.team_id
                  WHERE pt.project_id = p.id) AS teams
         FROM tickets_project p JOIN accounts_user u ON u.id = p.owner_id
         WHERE NOT EXISTS (SELECT 1 FROM tickets_project_teams pt JOIN t_team_membership tm ON tm.team_id = pt.team_id
                           WHERE pt.project_id = p.id AND tm.user_id = p.owner_id AND tm.role = 'admin' AND tm.scoped_project_id IS NULL)
         ORDER BY p.prefix",
    )
    .await?;
    // 7. キーの利用状況(個人キーの最終利用・共有 AI キーの書き込み。直近 30 日)
    let personal_keys = rows_json(
        pool,
        "SELECT k.id, u.username, k.key_prefix AS \"keyPrefix\", k.label, k.last_used_at AS \"lastUsedAt\"
         FROM ai_agent_api_keys k JOIN accounts_user u ON u.id = k.user_id
         WHERE k.revoked_at IS NULL ORDER BY k.last_used_at DESC NULLS LAST",
    )
    .await?;
    let shared_ai_writes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_audit_log
         WHERE action = 'shared_ai_key_write' AND created_at > NOW() - INTERVAL '30 days'",
    )
    .fetch_one(pool)
    .await?;
    let integrations = rows_json(
        pool,
        "SELECT i.id, i.name, i.is_active AS \"isActive\",
                (SELECT json_agg(m.name ORDER BY m.name) FROM access_integration_team it JOIN m_team m ON m.id = it.team_id
                  WHERE it.integration_id = i.id) AS teams
         FROM access_integration i ORDER BY i.name",
    )
    .await?;

    Ok(json!({
        "teams": teams,
        "staffUsers": staff,
        "projectOnlyUsers": project_only,
        "emailDomains": domains,
        "internalDomains": internal,
        "externalDomainUsers": external_users,
        "teamlessTickets": teamless,
        "projectOwnersNotTeamOwners": owners_not_team_owners,
        "keys": {
            "personalKeys": personal_keys,
            "sharedAiKeyWritesLast30Days": shared_ai_writes,
            "integrations": integrations,
            // 今の共有の外部 API キー(SENN_API_KEY)は利用の記録が無い。利用元は運用側で確認する
            "sharedExternalApiKeyConfigured": std::env::var("SENN_API_KEY").map(|v| !v.is_empty()).unwrap_or(false),
        },
    }))
}

/// GET /api/v1/system-admin/access-migration-report/
pub async fn access_migration_report(State(state): State<AppState>, viewer: Viewer) -> Response {
    if !viewer.is_system_admin() || viewer.principal.is_key() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "detail": "システム管理者だけが使えます" })),
        )
            .into_response();
    }
    match build(&state.pool).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            tracing::error!("[移行の確認] 一覧の作成に失敗: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "detail": "サーバーエラーが発生しました" })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_pool;

    /// すべての SQL が、実 DB で通ること(列名の誤りを捕まえる)
    #[tokio::test]
    async fn report_queries_run() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let v = build(&pool).await.unwrap();
        for key in [
            "teams",
            "staffUsers",
            "projectOnlyUsers",
            "emailDomains",
            "teamlessTickets",
            "projectOwnersNotTeamOwners",
        ] {
            assert!(v[key].is_array(), "{key}");
        }
        assert!(v["keys"]["personalKeys"].is_array());
    }
}
