//! チームの参加・公開区分・Owner と、孤立 Private チームの救済(アクセス制御の再設計 G-1・G-4・G-5)
//!
//! 詳細設計書 §11.1・§11.4。新しい機能なので、試運転のスイッチは通さず、最初から新しい規則(policy)で動く
//! (今の規則でも、所属していない人は Private チームの中身を見られないので、矛盾しない)。
//!
//! - POST   /api/v1/teams/{id}/join/                        Join(Full Member・Public のみ)
//! - POST   /api/v1/teams/{id}/leave/                       退出(Private の最後の Full Member・owners の最後の Owner は 409)
//! - PATCH  /api/v1/teams/{id}/access/                      公開区分・設定の方針(Private → Public は confirm 必須)
//! - POST   /api/v1/teams/{id}/owners/{user_id}/            Owner の指名
//! - DELETE /api/v1/teams/{id}/owners/{user_id}/            Owner の解除(owners の最後の Owner は 409)
//! - GET    /api/v1/system-admin/orphan-teams/              救済の対象の Private チーム(孤立・Owner 不在)の一覧(名前・メンバー数だけ)
//! - POST   /api/v1/system-admin/orphan-teams/{id}/rescue/  Owner を 1 人指名(監査記録)
//!
//! Owner の指名・解除、設定の方針・公開区分の変更は「Owner の操作」(Owner とシステム管理者だけ。
//! 設定の方針にかかわらない。設計書 §4.3・詳細設計書 §5.4。DEMO-000169)。
//!
//! 変更はすべて `access_audit_log` に記録する。

// 失敗は、そのまま応答として返すため、Err に Response を持つ
#![allow(clippy::result_large_err)]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};

use crate::domain::access::{
    can, Action, Decision, ResourceRef, SettingsPolicy, TeamFacts, Viewer, Visibility,
};
use crate::infrastructure::access::facts_repo;
use crate::presentation::state::AppState;

fn reply(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({ "detail": detail }))).into_response()
}

fn conflict(code: &str, detail: &str) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({ "detail": detail, "code": code })),
    )
        .into_response()
}

fn server_error(e: impl std::fmt::Debug) -> Response {
    tracing::error!("[チームの公開区分] DB 操作に失敗: {:?}", e);
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

fn decision_to_result(d: Decision) -> Result<(), Response> {
    match d {
        Decision::Allow => Ok(()),
        Decision::NotFound => Err(reply(StatusCode::NOT_FOUND, "見つかりません")),
        Decision::Forbidden => Err(reply(StatusCode::FORBIDDEN, "この操作の権限がありません")),
    }
}

/// チームの情報を読み、操作の判定をする(見えなければ 404、見えるが不可なら 403)
async fn team_and_check(
    pool: &PgPool,
    viewer: &Viewer,
    team_id: i32,
    action: Action,
) -> Result<TeamFacts, Response> {
    let team = facts_repo::facts_for_team(pool, team_id)
        .await
        .map_err(server_error)?
        .ok_or_else(|| reply(StatusCode::NOT_FOUND, "見つかりません"))?;
    decision_to_result(can(viewer, action, &ResourceRef::Team(team)))?;
    Ok(team)
}

/// 監査記録(本文・秘密は入れない)
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    viewer: &Viewer,
    action: &str,
    team_id: i32,
    target_user_id: Option<i32>,
    detail: Value,
) -> sqlx::Result<()> {
    let kind = if viewer.principal.is_key() {
        "personal_key"
    } else {
        "human"
    };
    sqlx::query(
        "INSERT INTO access_audit_log (actor_user_id, actor_kind, action, team_id, target_user_id, detail)
         VALUES ($1::int8, $2, $3, $4::int8, $5::int8, $6)",
    )
    .bind(viewer.user_id().map(|u| u as i64))
    .bind(kind)
    .bind(action)
    .bind(team_id as i64)
    .bind(target_user_id.map(|u| u as i64))
    .bind(detail)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// チーム全体の所属を持つ、有効な Full Member の人数(Guest・無効化を除く)
async fn full_member_count(
    tx: &mut Transaction<'_, Postgres>,
    team_id: i32,
    except_user: Option<i32>,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(DISTINCT tm.user_id) FROM t_team_membership tm
         JOIN accounts_user u ON u.id = tm.user_id
         WHERE tm.team_id = $1::int8 AND tm.scoped_project_id IS NULL
           AND u.is_active AND NOT u.is_guest
           AND ($2::int8 IS NULL OR tm.user_id <> $2::int8)",
    )
    .bind(team_id as i64)
    .bind(except_user.map(|u| u as i64))
    .fetch_one(&mut **tx)
    .await
}

// =============================================================================
// G-1: Join・退出
// =============================================================================

/// POST /api/v1/teams/{id}/join/
pub async fn join(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_id): Path<i32>,
) -> Response {
    let Some(user_id) = viewer.user_id() else {
        return reply(StatusCode::FORBIDDEN, "この操作の権限がありません");
    };
    if let Err(resp) = team_and_check(&state.pool, &viewer, team_id, Action::Join).await {
        return resp;
    }
    let result: sqlx::Result<()> = async {
        let mut tx = state.pool.begin().await?;
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at)
             SELECT $1::int8, $2::int8, 'member', NOW()
             WHERE NOT EXISTS (SELECT 1 FROM t_team_membership
                               WHERE team_id = $1::int8 AND user_id = $2::int8 AND scoped_project_id IS NULL)",
        )
        .bind(team_id as i64)
        .bind(user_id as i64)
        .execute(&mut *tx)
        .await?;
        audit(&mut tx, &viewer, "team.joined", team_id, Some(user_id), json!({})).await?;
        tx.commit().await
    }
    .await;
    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "teamId": team_id, "joined": true })),
        )
            .into_response(),
        Err(e) => server_error(e),
    }
}

/// POST /api/v1/teams/{id}/leave/
pub async fn leave(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_id): Path<i32>,
) -> Response {
    let Some(user_id) = viewer.user_id() else {
        return reply(StatusCode::FORBIDDEN, "この操作の権限がありません");
    };
    let team = match team_and_check(&state.pool, &viewer, team_id, Action::Read).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    let Some(membership) = viewer.team_membership(team_id).cloned() else {
        return reply(StatusCode::BAD_REQUEST, "このチームに参加していません");
    };
    let result: Result<(), Response> = async {
        let mut tx = state.pool.begin().await.map_err(server_error)?;
        // Private の最後の Full Member は抜けられない(チームが孤立する)
        if team.visibility == Visibility::Private && viewer.is_full_member() {
            let others = full_member_count(&mut tx, team_id, Some(user_id))
                .await
                .map_err(server_error)?;
            if others == 0 {
                return Err(conflict(
                    "last_member",
                    "Private チームの最後のメンバーは退出できません。先に別のメンバーを追加してください",
                ));
            }
        }
        // owners の方針で、最後の Owner は抜けられない
        if team.settings_policy == SettingsPolicy::Owners
            && membership.is_owner
            && team.owner_count <= 1
        {
            return Err(conflict(
                "last_owner",
                "最後の Owner は退出できません。先に別の人を Owner にしてください",
            ));
        }
        sqlx::query(
            "DELETE FROM t_team_membership WHERE team_id = $1::int8 AND user_id = $2::int8 AND scoped_project_id IS NULL",
        )
        .bind(team_id as i64)
        .bind(user_id as i64)
        .execute(&mut *tx)
        .await
        .map_err(server_error)?;
        audit(&mut tx, &viewer, "team.left", team_id, Some(user_id), json!({}))
            .await
            .map_err(server_error)?;
        tx.commit().await.map_err(server_error)
    }
    .await;
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(resp) => resp,
    }
}

// =============================================================================
// G-1・G-5: 公開区分・設定の方針
// =============================================================================

#[derive(Deserialize)]
pub struct TeamAccessIn {
    pub visibility: Option<String>,
    #[serde(rename = "settingsPolicy")]
    pub settings_policy: Option<String>,
    /// Private → Public の確認(全員に公開される)
    #[serde(default)]
    pub confirm: bool,
}

/// PATCH /api/v1/teams/{id}/access/
pub async fn update_access(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_id): Path<i32>,
    Json(body): Json<TeamAccessIn>,
) -> Response {
    let visibility = match body.visibility.as_deref() {
        None => None,
        Some("public") => Some(Visibility::Public),
        Some("private") => Some(Visibility::Private),
        Some(_) => {
            return reply(
                StatusCode::BAD_REQUEST,
                "visibility は public か private を指定してください",
            )
        }
    };
    let policy = match body.settings_policy.as_deref() {
        None => None,
        Some("members") => Some(SettingsPolicy::Members),
        Some("owners") => Some(SettingsPolicy::Owners),
        Some(_) => {
            return reply(
                StatusCode::BAD_REQUEST,
                "settingsPolicy は members か owners を指定してください",
            )
        }
    };
    if visibility.is_none() && policy.is_none() {
        return reply(
            StatusCode::BAD_REQUEST,
            "visibility か settingsPolicy を指定してください",
        );
    }
    let action = if visibility.is_some() {
        Action::ChangeVisibility
    } else {
        Action::ManageOwners
    };
    let team = match team_and_check(&state.pool, &viewer, team_id, action).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    if policy.is_some() {
        if let Err(resp) =
            decision_to_result(can(&viewer, Action::ManageOwners, &ResourceRef::Team(team)))
        {
            return resp;
        }
    }
    let to_public =
        team.visibility == Visibility::Private && visibility == Some(Visibility::Public);
    let to_private =
        team.visibility == Visibility::Public && visibility == Some(Visibility::Private);
    if to_public && !body.confirm {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "detail": "Private から Public にすると、社内の全員がこのチームの中身を見られるようになります。確認のうえ confirm: true で送ってください",
                "code": "confirm_required",
            })),
        )
            .into_response();
    }
    let result: sqlx::Result<()> = async {
        let mut tx = state.pool.begin().await?;
        if let Some(v) = visibility {
            sqlx::query("UPDATE m_team SET visibility = $2 WHERE id = $1::int8")
                .bind(team_id as i64)
                .bind(match v {
                    Visibility::Public => "public",
                    Visibility::Private => "private",
                })
                .execute(&mut *tx)
                .await?;
        }
        if let Some(p) = policy {
            sqlx::query("UPDATE m_team SET settings_policy = $2 WHERE id = $1::int8")
                .bind(team_id as i64)
                .bind(match p {
                    SettingsPolicy::Members => "members",
                    SettingsPolicy::Owners => "owners",
                })
                .execute(&mut *tx)
                .await?;
        }
        let removed = if to_private {
            cleanup_after_private(&mut tx, team_id).await?
        } else {
            json!({})
        };
        audit(
            &mut tx,
            &viewer,
            "team.access_changed",
            team_id,
            None,
            json!({
                "visibility": body.visibility,
                "settingsPolicy": body.settings_policy,
                "removed": removed,
            }),
        )
        .await?;
        tx.commit().await
    }
    .await;
    match result {
        Ok(()) => {
            let facts = match facts_repo::facts_for_team(&state.pool, team_id).await {
                Ok(Some(f)) => f,
                Ok(None) => return reply(StatusCode::NOT_FOUND, "見つかりません"),
                Err(e) => return server_error(e),
            };
            (
                StatusCode::OK,
                Json(json!({
                    "teamId": team_id,
                    "visibility": match facts.visibility { Visibility::Public => "public", Visibility::Private => "private" },
                    "settingsPolicy": match facts.settings_policy { SettingsPolicy::Members => "members", SettingsPolicy::Owners => "owners" },
                })),
            )
                .into_response()
        }
        Err(e) => server_error(e),
    }
}

/// G-5: Public → Private にしたとき、見えなくなる人の、ウォッチ・未読の通知・保存済みビューを外す(設計書 §11.5)
///
/// 見える人 = チーム全体の所属、またはそのチケットのプロジェクトのプロジェクト単位の所属(同じチーム)。
/// 戻り値は件数(監査記録に残す)。
async fn cleanup_after_private(
    tx: &mut Transaction<'_, Postgres>,
    team_id: i32,
) -> sqlx::Result<Value> {
    // チケット t を、利用者 x が見られるか(チームの規則。期限の切れたプロジェクト単位の所属も含めて残す側に倒す)
    const SEES: &str = "EXISTS (SELECT 1 FROM t_team_membership m
                         WHERE m.team_id = t.team_id AND m.user_id = {user}
                           AND (m.scoped_project_id IS NULL OR m.scoped_project_id = t.project_id))";
    let watchers = sqlx::query(&format!(
        "DELETE FROM tickets_ticket_watchers w USING tickets_ticket t
         WHERE w.ticketmodel_id = t.id AND t.team_id = $1::int8 AND NOT {}",
        SEES.replace("{user}", "w.user_id")
    ))
    .bind(team_id as i64)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    let notifications = sqlx::query(&format!(
        "DELETE FROM notifications_notification n USING tickets_ticket t
         WHERE n.ticket_id = t.id AND t.team_id = $1::int8 AND NOT n.is_read AND NOT {}",
        SEES.replace("{user}", "n.user_id")
    ))
    .bind(team_id as i64)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    let saved_views = sqlx::query(
        "DELETE FROM t_saved_view v
         WHERE v.team_id = $1::int8
           AND NOT EXISTS (SELECT 1 FROM t_team_membership m
                           WHERE m.team_id = v.team_id AND m.user_id = v.owner_id AND m.scoped_project_id IS NULL)",
    )
    .bind(team_id as i64)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(json!({
        "watchers": watchers,
        "unreadNotifications": notifications,
        "savedViews": saved_views,
    }))
}

// =============================================================================
// G-1: Owner の指名・解除
// =============================================================================

/// POST /api/v1/teams/{id}/owners/{user_id}/
pub async fn add_owner(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((team_id, target)): Path<(i32, i32)>,
) -> Response {
    if let Err(resp) = team_and_check(&state.pool, &viewer, team_id, Action::ManageOwners).await {
        return resp;
    }
    let result: Result<(), Response> = async {
        let mut tx = state.pool.begin().await.map_err(server_error)?;
        // Owner にできるのは、チーム全体の所属を持つ、有効な Full Member だけ
        let updated = sqlx::query(
            "UPDATE t_team_membership tm SET role = 'admin'
             FROM accounts_user u
             WHERE u.id = tm.user_id AND tm.team_id = $1::int8 AND tm.user_id = $2::int8
               AND tm.scoped_project_id IS NULL AND u.is_active AND NOT u.is_guest",
        )
        .bind(team_id as i64)
        .bind(target as i64)
        .execute(&mut *tx)
        .await
        .map_err(server_error)?
        .rows_affected();
        if updated == 0 {
            return Err(reply(
                StatusCode::BAD_REQUEST,
                "Owner にできるのは、このチームのメンバー(Guest 以外)だけです",
            ));
        }
        audit(
            &mut tx,
            &viewer,
            "team.owner_added",
            team_id,
            Some(target),
            json!({}),
        )
        .await
        .map_err(server_error)?;
        tx.commit().await.map_err(server_error)
    }
    .await;
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(resp) => resp,
    }
}

/// DELETE /api/v1/teams/{id}/owners/{user_id}/
pub async fn remove_owner(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((team_id, target)): Path<(i32, i32)>,
) -> Response {
    let team = match team_and_check(&state.pool, &viewer, team_id, Action::ManageOwners).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    let result: Result<(), Response> = async {
        let mut tx = state.pool.begin().await.map_err(server_error)?;
        let is_owner: Option<bool> = sqlx::query_scalar(
            "SELECT role = 'admin' FROM t_team_membership
             WHERE team_id = $1::int8 AND user_id = $2::int8 AND scoped_project_id IS NULL",
        )
        .bind(team_id as i64)
        .bind(target as i64)
        .fetch_optional(&mut *tx)
        .await
        .map_err(server_error)?;
        if is_owner != Some(true) {
            return Err(reply(StatusCode::BAD_REQUEST, "この人は Owner ではありません"));
        }
        if team.settings_policy == SettingsPolicy::Owners && team.owner_count <= 1 {
            return Err(conflict(
                "last_owner",
                "最後の Owner は解除できません。先に別の人を Owner にするか、設定の方針を「メンバー全員」にしてください",
            ));
        }
        sqlx::query(
            "UPDATE t_team_membership SET role = 'member'
             WHERE team_id = $1::int8 AND user_id = $2::int8 AND scoped_project_id IS NULL",
        )
        .bind(team_id as i64)
        .bind(target as i64)
        .execute(&mut *tx)
        .await
        .map_err(server_error)?;
        audit(&mut tx, &viewer, "team.owner_removed", team_id, Some(target), json!({}))
            .await
            .map_err(server_error)?;
        tx.commit().await.map_err(server_error)
    }
    .await;
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(resp) => resp,
    }
}

// =============================================================================
// G-4: 孤立 Private チームの救済(システム管理者)
// =============================================================================

/// 孤立 = Private で、有効な Full Member のチーム全体の所属が 0 件
const ORPHAN_WHERE: &str = "m.visibility = 'private'
    AND NOT EXISTS (SELECT 1 FROM t_team_membership tm JOIN accounts_user u ON u.id = tm.user_id
                    WHERE tm.team_id = m.id AND tm.scoped_project_id IS NULL AND u.is_active AND NOT u.is_guest)";

/// Owner 不在 = Private で、有効な Full Member の所属はあるが、そのうち Owner(admin)が 0 件(設計書 §4.4。
/// Owner の指名はシステム管理者だけになったため、救済の対象にする。DEMO-000169)
const NO_OWNER_WHERE: &str = "m.visibility = 'private'
    AND EXISTS (SELECT 1 FROM t_team_membership tm JOIN accounts_user u ON u.id = tm.user_id
                WHERE tm.team_id = m.id AND tm.scoped_project_id IS NULL AND u.is_active AND NOT u.is_guest)
    AND NOT EXISTS (SELECT 1 FROM t_team_membership tm JOIN accounts_user u ON u.id = tm.user_id
                    WHERE tm.team_id = m.id AND tm.scoped_project_id IS NULL AND tm.role = 'admin'
                      AND u.is_active AND NOT u.is_guest)";

/// 救済の種類。孤立なら誰でも(自分を含む)、Owner 不在なら今のメンバーからだけ指名できる
async fn rescue_kind(
    tx: &mut Transaction<'_, Postgres>,
    team_id: i32,
) -> sqlx::Result<Option<&'static str>> {
    let kind: Option<String> = sqlx::query_scalar(&format!(
        "SELECT CASE WHEN {ORPHAN_WHERE} THEN 'no_members' ELSE 'no_owner' END
         FROM m_team m WHERE m.id = $1::int8 AND (({ORPHAN_WHERE}) OR ({NO_OWNER_WHERE}))"
    ))
    .bind(team_id as i64)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(match kind.as_deref() {
        Some("no_members") => Some("no_members"),
        Some(_) => Some("no_owner"),
        None => None,
    })
}

fn require_system_admin(viewer: &Viewer) -> Result<(), Response> {
    if viewer.is_system_admin() && !viewer.principal.is_key() {
        Ok(())
    } else {
        Err(reply(StatusCode::FORBIDDEN, "システム管理者だけが使えます"))
    }
}

/// GET /api/v1/system-admin/orphan-teams/ — 名前・メンバー数・種類だけを返す(中身は見せない)。
/// Owner 不在(`no_owner`)は、指名の候補として、メンバーの ID と表示名も返す(`candidates`)
pub async fn orphan_teams(State(state): State<AppState>, viewer: Viewer) -> Response {
    if let Err(resp) = require_system_admin(&viewer) {
        return resp;
    }
    let result: sqlx::Result<Vec<Value>> = async {
        let rows: Vec<(i32, String, i64, bool)> = sqlx::query_as(&format!(
            "SELECT m.id::int4, m.name,
                    (SELECT COUNT(*) FROM t_team_membership tm WHERE tm.team_id = m.id)::int8,
                    ({ORPHAN_WHERE})
             FROM m_team m WHERE ({ORPHAN_WHERE}) OR ({NO_OWNER_WHERE}) ORDER BY m.name"
        ))
        .fetch_all(&state.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for (id, name, member_count, no_members) in rows {
            if no_members {
                out.push(json!({ "id": id, "name": name, "memberCount": member_count, "kind": "no_members" }));
                continue;
            }
            let candidates: Vec<(i32, String)> = sqlx::query_as(
                "SELECT u.id::int4, COALESCE(NULLIF(u.display_name, ''), u.username)
                 FROM t_team_membership tm JOIN accounts_user u ON u.id = tm.user_id
                 WHERE tm.team_id = $1::int8 AND tm.scoped_project_id IS NULL
                   AND u.is_active AND NOT u.is_guest
                 ORDER BY 2",
            )
            .bind(id as i64)
            .fetch_all(&state.pool)
            .await?;
            out.push(json!({
                "id": id,
                "name": name,
                "memberCount": member_count,
                "kind": "no_owner",
                "candidates": candidates
                    .into_iter()
                    .map(|(uid, display)| json!({ "id": uid, "displayName": display }))
                    .collect::<Vec<_>>(),
            }));
        }
        Ok(out)
    }
    .await;
    match result {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => server_error(e),
    }
}

#[derive(Deserialize)]
pub struct RescueIn {
    #[serde(rename = "userId")]
    pub user_id: i32,
}

/// POST /api/v1/system-admin/orphan-teams/{id}/rescue/ — 指名した人を Owner として所属させる
pub async fn rescue_orphan(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(team_id): Path<i32>,
    Json(body): Json<RescueIn>,
) -> Response {
    if let Err(resp) = require_system_admin(&viewer) {
        return resp;
    }
    let result: Result<(), Response> = async {
        let mut tx = state.pool.begin().await.map_err(server_error)?;
        let Some(kind) = rescue_kind(&mut tx, team_id).await.map_err(server_error)? else {
            // 救済の対象でない(または存在しない)チームの中身・所属は、システム管理者でも触らない
            return Err(reply(
                StatusCode::NOT_FOUND,
                "救済の対象の Private チームが見つかりません",
            ));
        };
        let eligible: bool = if kind == "no_members" {
            sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM accounts_user WHERE id = $1::int8 AND is_active AND NOT is_guest)",
            )
            .bind(body.user_id as i64)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_error)?
        } else {
            // Owner 不在: 今のメンバーからだけ(システム管理者が自分を入れて中を見る道を作らない)
            sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM t_team_membership tm JOIN accounts_user u ON u.id = tm.user_id
                 WHERE tm.team_id = $1::int8 AND tm.user_id = $2::int8 AND tm.scoped_project_id IS NULL
                   AND u.is_active AND NOT u.is_guest)",
            )
            .bind(team_id as i64)
            .bind(body.user_id as i64)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_error)?
        };
        if !eligible {
            return Err(reply(
                StatusCode::BAD_REQUEST,
                if kind == "no_members" {
                    "Owner に指名できるのは、有効な Full Member だけです"
                } else {
                    "Owner に指名できるのは、このチームの今のメンバー(Guest 以外)だけです"
                },
            ));
        }
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at)
             VALUES ($1::int8, $2::int8, 'admin', NOW())
             ON CONFLICT DO NOTHING",
        )
        .bind(team_id as i64)
        .bind(body.user_id as i64)
        .execute(&mut *tx)
        .await
        .map_err(server_error)?;
        sqlx::query(
            "UPDATE t_team_membership SET role = 'admin'
             WHERE team_id = $1::int8 AND user_id = $2::int8 AND scoped_project_id IS NULL",
        )
        .bind(team_id as i64)
        .bind(body.user_id as i64)
        .execute(&mut *tx)
        .await
        .map_err(server_error)?;
        audit(&mut tx, &viewer, "team.rescued", team_id, Some(body.user_id), json!({ "kind": kind }))
            .await
            .map_err(server_error)?;
        tx.commit().await.map_err(server_error)
    }
    .await;
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(resp) => resp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        create_test_project, create_test_ticket, create_test_user, test_pool,
    };

    async fn run_sql(pool: &PgPool, sql: &str, binds: &[i64]) {
        let mut q = sqlx::query(sql);
        for b in binds {
            q = q.bind(*b);
        }
        q.execute(pool).await.unwrap();
    }

    async fn count(pool: &PgPool, sql: &str, binds: &[i64]) -> i64 {
        let mut q = sqlx::query_scalar(sql);
        for b in binds {
            q = q.bind(*b);
        }
        q.fetch_one(pool).await.unwrap()
    }

    /// G-5: Public → Private で、見えなくなる人のウォッチ・未読通知・保存済みビューだけを外す
    #[tokio::test]
    async fn cleanup_after_private_removes_only_outsiders() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let member = create_test_user(&pool, "gcl-m").await as i64;
        let outsider = create_test_user(&pool, "gcl-o").await as i64;
        let project = create_test_project(&pool, "GCL", member as i32).await;
        let ticket = create_test_ticket(&pool, project, "GCL", member as i32).await as i64;
        let team: i64 = count(
            &pool,
            "SELECT team_id FROM tickets_ticket WHERE id = $1::int8",
            &[ticket],
        )
        .await;
        run_sql(&pool, "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1::int8, $2::int8, 'member', NOW())", &[team, member]).await;
        for u in [member, outsider] {
            run_sql(&pool, "INSERT INTO tickets_ticket_watchers (ticketmodel_id, user_id) VALUES ($1::int8, $2::int8)", &[ticket, u]).await;
            run_sql(&pool, "INSERT INTO notifications_notification (user_id, ticket_id, category, title, message, is_read, created_at) VALUES ($1::int8, $2::int8, 'ticket', 't', 'm', false, NOW())", &[u, ticket]).await;
            run_sql(&pool, "INSERT INTO t_saved_view (project_id, team_id, owner_id, name, filters, view_type, is_shared, created_at, updated_at) VALUES (NULL, $1::int8, $2::int8, 'v', '{}', 'tickets', false, NOW(), NOW())", &[team, u]).await;
        }

        let mut tx = pool.begin().await.unwrap();
        let removed = cleanup_after_private(&mut tx, team as i32).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(removed["watchers"], 1);
        assert_eq!(removed["unreadNotifications"], 1);
        assert_eq!(removed["savedViews"], 1);

        for (u, want) in [(member, 1), (outsider, 0)] {
            assert_eq!(count(&pool, "SELECT COUNT(*) FROM tickets_ticket_watchers WHERE ticketmodel_id = $1::int8 AND user_id = $2::int8", &[ticket, u]).await, want);
            assert_eq!(count(&pool, "SELECT COUNT(*) FROM notifications_notification WHERE ticket_id = $1::int8 AND user_id = $2::int8", &[ticket, u]).await, want);
            assert_eq!(count(&pool, "SELECT COUNT(*) FROM t_saved_view WHERE team_id = $1::int8 AND owner_id = $2::int8", &[team, u]).await, want);
        }
    }

    /// G-4: 孤立 = Private で、有効な Full Member のチーム全体の所属が 0 件(Guest だけのチームは孤立)
    #[tokio::test]
    async fn orphan_means_private_without_active_full_members() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = crate::test_support::create_test_team(&pool, "gorph").await as i64;
        let guest = create_test_user(&pool, "gorph-g").await as i64;
        let member = create_test_user(&pool, "gorph-m").await as i64;
        run_sql(
            &pool,
            "UPDATE accounts_user SET is_guest = true WHERE id = $1::int8",
            &[guest],
        )
        .await;
        run_sql(&pool, "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1::int8, $2::int8, 'member', NOW())", &[team, guest]).await;
        let is_orphan = |pool: PgPool| async move {
            sqlx::query_scalar::<_, bool>(&format!(
                "SELECT EXISTS (SELECT 1 FROM m_team m WHERE m.id = $1::int8 AND {ORPHAN_WHERE})"
            ))
            .bind(team)
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        assert!(!is_orphan(pool.clone()).await, "Public は孤立ではない");
        run_sql(
            &pool,
            "UPDATE m_team SET visibility = 'private' WHERE id = $1::int8",
            &[team],
        )
        .await;
        assert!(is_orphan(pool.clone()).await, "Guest だけの Private は孤立");
        run_sql(&pool, "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1::int8, $2::int8, 'member', NOW())", &[team, member]).await;
        assert!(
            !is_orphan(pool.clone()).await,
            "Full Member がいれば孤立ではない"
        );
        run_sql(
            &pool,
            "UPDATE accounts_user SET is_active = false WHERE id = $1::int8",
            &[member],
        )
        .await;
        assert!(
            is_orphan(pool.clone()).await,
            "無効化された人しかいなければ孤立"
        );
    }
}

/// Owner の規則(設計書 §4.3・§4.4、詳細設計書 §5.4・§11.4。DEMO-000169)
#[cfg(test)]
mod owner_tests {
    use super::*;
    use crate::test_support::{
        add_test_team_member, create_test_team, create_test_user, test_pool, test_state,
        test_viewer,
    };

    async fn role_of(pool: &PgPool, team: i32, user: i32) -> Option<String> {
        sqlx::query_scalar(
            "SELECT role FROM t_team_membership WHERE team_id = $1::int8 AND user_id = $2::int8 AND scoped_project_id IS NULL",
        )
        .bind(team as i64)
        .bind(user as i64)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    async fn settings_policy_of(pool: &PgPool, team: i32) -> String {
        sqlx::query_scalar("SELECT settings_policy FROM m_team WHERE id = $1::int8")
            .bind(team as i64)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn system_admin(pool: &PgPool) -> i32 {
        let u = create_test_user(pool, "own-sa").await;
        sqlx::query("UPDATE accounts_user SET is_staff = true WHERE id = $1::int8")
            .bind(u as i64)
            .execute(pool)
            .await
            .unwrap();
        u
    }

    async fn call_add_owner(pool: &PgPool, as_user: i32, team: i32, target: i32) -> StatusCode {
        add_owner(
            State(test_state(pool).await),
            test_viewer(pool, as_user).await,
            Path((team, target)),
        )
        .await
        .status()
    }

    async fn call_remove_owner(pool: &PgPool, as_user: i32, team: i32, target: i32) -> StatusCode {
        remove_owner(
            State(test_state(pool).await),
            test_viewer(pool, as_user).await,
            Path((team, target)),
        )
        .await
        .status()
    }

    async fn call_access(pool: &PgPool, as_user: i32, team: i32, body: Value) -> StatusCode {
        update_access(
            State(test_state(pool).await),
            test_viewer(pool, as_user).await,
            Path(team),
            Json(serde_json::from_value(body).unwrap()),
        )
        .await
        .status()
    }

    /// 本番で起きうる乗っ取り: Public(members)チームに Join した人が、自分を Owner にし、
    /// 方針を owners にして、元の Owner を外す。どの段階も拒否されること
    #[tokio::test]
    async fn joiner_cannot_take_over_team() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "own-take").await;
        let owner = create_test_user(&pool, "own-o").await;
        let joiner = create_test_user(&pool, "own-j").await;
        add_test_team_member(&pool, team, owner, "admin").await;

        let status = join(
            State(test_state(&pool).await),
            test_viewer(&pool, joiner).await,
            Path(team),
        )
        .await
        .status();
        assert_eq!(status, StatusCode::OK, "Public チームには Join できる");

        assert_eq!(
            call_add_owner(&pool, joiner, team, joiner).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            role_of(&pool, team, joiner).await.as_deref(),
            Some("member")
        );
        assert_eq!(
            call_access(&pool, joiner, team, json!({ "settingsPolicy": "owners" })).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(settings_policy_of(&pool, team).await, "members");
        assert_eq!(
            call_access(&pool, joiner, team, json!({ "visibility": "private" })).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call_remove_owner(&pool, joiner, team, owner).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(role_of(&pool, team, owner).await.as_deref(), Some("admin"));

        // Owner は、今までどおり指名・方針の変更ができる
        assert_eq!(
            call_add_owner(&pool, owner, team, joiner).await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call_access(&pool, owner, team, json!({ "settingsPolicy": "owners" })).await,
            StatusCode::OK
        );
    }

    /// Owner が 0 人のチームでは、メンバーは Owner を指名できず、システム管理者だけができる
    #[tokio::test]
    async fn ownerless_team_needs_system_admin_to_nominate() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "own-none").await;
        let m = create_test_user(&pool, "own-m").await;
        add_test_team_member(&pool, team, m, "member").await;
        let sa = system_admin(&pool).await;

        assert_eq!(
            call_add_owner(&pool, m, team, m).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call_add_owner(&pool, sa, team, m).await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(role_of(&pool, team, m).await.as_deref(), Some("admin"));
    }

    /// 退出: owners の方針の最後の Owner、Private の最後のメンバーは抜けられない(チームを詰まらせない)
    #[tokio::test]
    async fn last_owner_and_last_private_member_cannot_leave() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let call_leave = |user: i32, team: i32| {
            let pool = pool.clone();
            async move {
                leave(
                    State(test_state(&pool).await),
                    test_viewer(&pool, user).await,
                    Path(team),
                )
                .await
                .status()
            }
        };
        let owners_team = create_test_team(&pool, "own-lv").await;
        sqlx::query("UPDATE m_team SET settings_policy = 'owners' WHERE id = $1::int8")
            .bind(owners_team as i64)
            .execute(&pool)
            .await
            .unwrap();
        let owner = create_test_user(&pool, "own-lo").await;
        add_test_team_member(&pool, owners_team, owner, "admin").await;
        assert_eq!(call_leave(owner, owners_team).await, StatusCode::CONFLICT);
        assert_eq!(
            role_of(&pool, owners_team, owner).await.as_deref(),
            Some("admin")
        );

        let private_team = create_test_team(&pool, "own-lp").await;
        sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1::int8")
            .bind(private_team as i64)
            .execute(&pool)
            .await
            .unwrap();
        let m = create_test_user(&pool, "own-lm").await;
        add_test_team_member(&pool, private_team, m, "member").await;
        assert_eq!(call_leave(m, private_team).await, StatusCode::CONFLICT);
        assert_eq!(
            role_of(&pool, private_team, m).await.as_deref(),
            Some("member")
        );
    }

    /// 救済の一覧・指名は、システム管理者でなければ使えない
    #[tokio::test]
    async fn rescue_is_for_system_admins_only() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "own-rx").await;
        sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1::int8")
            .bind(team as i64)
            .execute(&pool)
            .await
            .unwrap();
        let m = create_test_user(&pool, "own-rxm").await;
        add_test_team_member(&pool, team, m, "member").await;

        let list = orphan_teams(State(test_state(&pool).await), test_viewer(&pool, m).await).await;
        assert_eq!(list.status(), StatusCode::FORBIDDEN);
        let status = rescue_orphan(
            State(test_state(&pool).await),
            test_viewer(&pool, m).await,
            Path(team),
            Json(RescueIn { user_id: m }),
        )
        .await
        .status();
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "メンバーが自分を Owner にできない"
        );
        assert_eq!(role_of(&pool, team, m).await.as_deref(), Some("member"));
    }

    /// Owner 不在の Private チームは、救済の対象になり、今のメンバーの中からだけ指名できる
    #[tokio::test]
    async fn rescue_private_team_without_owner_from_members_only() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "own-resc").await;
        sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1::int8")
            .bind(team as i64)
            .execute(&pool)
            .await
            .unwrap();
        let m = create_test_user(&pool, "own-rm").await;
        let outsider = create_test_user(&pool, "own-ro").await;
        add_test_team_member(&pool, team, m, "member").await;
        let sa = system_admin(&pool).await;

        let list = orphan_teams(State(test_state(&pool).await), test_viewer(&pool, sa).await).await;
        assert_eq!(list.status(), StatusCode::OK);
        let body = axum::body::to_bytes(list.into_body(), usize::MAX)
            .await
            .unwrap();
        let rows: Vec<Value> = serde_json::from_slice(&body).unwrap();
        let row = rows
            .iter()
            .find(|r| r["id"] == json!(team))
            .expect("Owner 不在の Private チームが一覧に出る");
        assert_eq!(row["kind"], "no_owner");
        assert_eq!(row["candidates"][0]["id"], json!(m));

        let rescue = |user: i32| {
            let pool = pool.clone();
            async move {
                rescue_orphan(
                    State(test_state(&pool).await),
                    test_viewer(&pool, sa).await,
                    Path(team),
                    Json(RescueIn { user_id: user }),
                )
                .await
                .status()
            }
        };
        assert_eq!(rescue(sa).await, StatusCode::BAD_REQUEST, "自分は入れない");
        assert_eq!(rescue(outsider).await, StatusCode::BAD_REQUEST);
        assert_eq!(role_of(&pool, team, sa).await, None);
        assert_eq!(rescue(m).await, StatusCode::NO_CONTENT);
        assert_eq!(role_of(&pool, team, m).await.as_deref(), Some("admin"));
    }
}
