//! 招待とメール確認(アクセス制御の再設計 フェーズ A-2・A-3。詳細設計書 §11.1)
//!
//! - POST   /api/v1/invitations/                  招待の作成(チームの設定を管理できる人。チームの無い Full Member の招待はシステム管理者)
//! - GET    /api/v1/invitations/?teamId=          未使用の招待の一覧(同上)
//! - POST   /api/v1/invitations/{id}/revoke/      招待の取り消し(同上)
//! - GET    /api/v1/invitations/{token}/          招待の内容の確認(公開。メールアドレスは一部だけ)
//! - POST   /api/v1/invitations/{token}/accept/   招待の受諾とアカウントの作成(公開)
//! - POST   /api/v1/auth/verify-email/            社内ドメインの自己登録のメール確認(公開)
//!
//! 新しい機能なので、試運転のスイッチは通さず、最初から新しい規則(policy)で動く。
//! トークンは保存しない(SHA-256 だけ)。受諾・確認は 1 回限り・期限つき。

#![allow(clippy::result_large_err)]

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::{can, Action, Decision, ResourceRef, Role, Viewer};
use crate::domain::services::{auth_service, jwt_service};
use crate::infrastructure::access::facts_repo;
use crate::infrastructure::repositories::invitation_repo::{
    self, AcceptInput, AcceptOutcome, InviteRole, NewInvitation,
};
use crate::presentation::public_url::mail_link_base;
use crate::presentation::state::AppState;

fn reply(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({ "detail": detail }))).into_response()
}

fn server_error(e: impl std::fmt::Debug) -> Response {
    tracing::error!("[招待] 処理に失敗: {:?}", e);
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

/// 招待の有効期間(日)。`SENN_INVITATION_TTL_DAYS`(既定 7)
fn invitation_ttl_days() -> i64 {
    std::env::var("SENN_INVITATION_TTL_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|d: &i64| *d > 0)
        .unwrap_or(7)
}

fn is_production() -> bool {
    std::env::var("ENV_NAME").as_deref() == Ok("production")
}

/// メールを送る。送れた(または SMTP の設定があり送信を試みた)なら true。
/// SMTP 未設定(DRY-RUN)なら false(本番以外では、画面に URL を返して手で渡せるようにする)
async fn send_mail(state: &AppState, to: &str, subject: &str, body: &str) -> bool {
    match &state.mail_sender {
        Some(sender) if sender.is_configured().await => {
            if let Err(e) = sender.send(to, subject, body).await {
                tracing::error!("[招待] メール送信に失敗: {:?}", e);
            }
            true
        }
        _ => {
            tracing::info!("[招待] メール [DRY-RUN] to={} subject={}", to, subject);
            false
        }
    }
}

/// 招待先のチームについて、招待できるか(チームの設定を管理できる人)。チームが無い招待はシステム管理者だけ
async fn check_invite(
    state: &AppState,
    viewer: &Viewer,
    team_id: Option<i32>,
) -> Result<(), Response> {
    if viewer.principal.is_key() {
        return Err(reply(StatusCode::FORBIDDEN, "キーでは招待できません"));
    }
    match team_id {
        None if viewer.role == Role::SystemAdmin => Ok(()),
        None => Err(reply(
            StatusCode::FORBIDDEN,
            "チームを指定しない招待は、システム管理者だけが行えます",
        )),
        Some(team_id) => {
            let team = facts_repo::facts_for_team(&state.pool, team_id)
                .await
                .map_err(server_error)?
                .ok_or_else(|| reply(StatusCode::NOT_FOUND, "見つかりません"))?;
            match can(viewer, Action::Invite, &ResourceRef::Team(team)) {
                Decision::Allow => Ok(()),
                Decision::NotFound => Err(reply(StatusCode::NOT_FOUND, "見つかりません")),
                Decision::Forbidden => Err(reply(
                    StatusCode::FORBIDDEN,
                    "このチームに招待する権限がありません",
                )),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateInvitationIn {
    pub email: String,
    pub role: String,
    #[serde(rename = "teamId")]
    pub team_id: Option<i32>,
    #[serde(rename = "scopedProjectId")]
    pub scoped_project_id: Option<i32>,
    #[serde(rename = "endDate")]
    pub end_date: Option<NaiveDate>,
}

/// POST /api/v1/invitations/
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    headers: HeaderMap,
    Json(body): Json<CreateInvitationIn>,
) -> Response {
    let email = body.email.trim().to_string();
    if email.len() > 254 || !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        return reply(StatusCode::BAD_REQUEST, "メールアドレスが正しくありません");
    }
    let Some(role) = InviteRole::parse(&body.role) else {
        return reply(StatusCode::BAD_REQUEST, "role は full_member か guest です");
    };
    if role == InviteRole::Guest && body.team_id.is_none() {
        return reply(StatusCode::BAD_REQUEST, "Guest の招待にはチームが必要です");
    }
    if body.scoped_project_id.is_some() && role != InviteRole::Guest {
        return reply(
            StatusCode::BAD_REQUEST,
            "プロジェクト単位の招待は Guest だけです",
        );
    }
    if let Err(resp) = check_invite(&state, &viewer, body.team_id).await {
        return resp;
    }
    // プロジェクト単位の Guest は、そのチームが参加しているプロジェクトだけ
    if let (Some(team_id), Some(project_id)) = (body.team_id, body.scoped_project_id) {
        let participates: Result<bool, _> = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM tickets_project_teams WHERE project_id = $1::int8 AND team_id = $2::int8)",
        )
        .bind(i64::from(project_id))
        .bind(i64::from(team_id))
        .fetch_one(&state.pool)
        .await;
        match participates {
            Ok(true) => {}
            Ok(false) => {
                return reply(
                    StatusCode::BAD_REQUEST,
                    "そのプロジェクトには、このチームが参加していません",
                )
            }
            Err(e) => return server_error(e),
        }
    }
    match invitation_repo::email_registered(&state.pool, &email).await {
        Ok(true) => {
            return reply(
                StatusCode::CONFLICT,
                "このメールアドレスは、すでに登録されています(チームへの追加はメンバーの画面から行ってください)",
            )
        }
        Ok(false) => {}
        Err(e) => return server_error(e),
    }

    let Some(inviter) = viewer.user_id() else {
        return reply(StatusCode::FORBIDDEN, "この操作の権限がありません");
    };
    let token = invitation_repo::new_token();
    let expires_at = Utc::now() + Duration::days(invitation_ttl_days());
    let input = NewInvitation {
        email: email.clone(),
        role,
        team_id: body.team_id,
        scoped_project_id: body.scoped_project_id,
        end_date: body.end_date,
        invited_by: inviter,
        expires_at,
    };
    let invitation =
        match invitation_repo::create(&state.pool, &input, &invitation_repo::hash_token(&token))
            .await
        {
            Ok(i) => i,
            Err(e) => return server_error(e),
        };
    let _ = sqlx::query(
        "INSERT INTO access_audit_log (actor_user_id, actor_kind, action, team_id, detail)
         VALUES ($1::int8, 'human', 'invitation_created', $2::int8, $3)",
    )
    .bind(i64::from(inviter))
    .bind(body.team_id.map(i64::from))
    .bind(json!({"invitation_id": invitation.id, "role": role.as_str()}))
    .execute(&state.pool)
    .await
    .map_err(|e| tracing::error!("[招待] 監査記録に失敗: {:?}", e));

    let path = format!("/invite/{token}");
    let url = format!("{}{}", mail_link_base(&headers, &state.config), path);
    let subject = "SENN への招待";
    let mail = format!(
        "SENN への招待が届いています。\n\n\
         以下のリンクから {days} 日以内にアカウントを作成してください。\n\n\
         {url}\n\n\
         心当たりがない場合は、このメールを無視してください。\n",
        days = invitation_ttl_days()
    );
    let sent = send_mail(&state, &email, subject, &mail).await;
    // SMTP 未設定の本番以外では、URL を返して手で渡せるようにする(本番では返さない)
    let invite_url = (!sent && !is_production()).then_some(path);
    (
        StatusCode::CREATED,
        Json(json!({
            "invitation": invitation,
            "emailSent": sent,
            "inviteUrl": invite_url,
        })),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(rename = "teamId")]
    pub team_id: Option<i32>,
}

/// GET /api/v1/invitations/?teamId=
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(q): Query<ListQuery>,
) -> Response {
    if let Err(resp) = check_invite(&state, &viewer, q.team_id).await {
        return resp;
    }
    match invitation_repo::list_pending(&state.pool, q.team_id).await {
        Ok(items) => (StatusCode::OK, Json(json!({ "results": items }))).into_response(),
        Err(e) => server_error(e),
    }
}

/// POST /api/v1/invitations/{id}/revoke/
pub async fn revoke(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(key): Path<String>,
) -> Response {
    let Ok(id) = key.parse::<i64>() else {
        return reply(StatusCode::NOT_FOUND, "見つかりません");
    };
    let team_id = match invitation_repo::find_target(&state.pool, id).await {
        Ok(Some((team_id, true))) => team_id,
        Ok(_) => return reply(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => return server_error(e),
    };
    if let Err(resp) = check_invite(&state, &viewer, team_id).await {
        return resp;
    }
    match invitation_repo::revoke(&state.pool, id).await {
        Ok(true) => (StatusCode::OK, Json(json!({ "revoked": true }))).into_response(),
        Ok(false) => reply(StatusCode::NOT_FOUND, "見つかりません"),
        Err(e) => server_error(e),
    }
}

const INVALID_INVITATION: &str =
    "招待が見つからないか、期限が切れています。招待した人に、もう一度送ってもらってください";

/// GET /api/v1/invitations/{token}/(公開)
pub async fn preview(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    if token.len() != 64 {
        return reply(StatusCode::NOT_FOUND, INVALID_INVITATION);
    }
    match invitation_repo::preview(&state.pool, &invitation_repo::hash_token(&token)).await {
        Ok(Some(p)) => (StatusCode::OK, Json(p)).into_response(),
        Ok(None) => reply(StatusCode::NOT_FOUND, INVALID_INVITATION),
        Err(e) => server_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct AcceptIn {
    pub username: String,
    pub password: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
}

/// POST /api/v1/invitations/{token}/accept/(公開)
pub async fn accept(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<AcceptIn>,
) -> Response {
    if token.len() != 64 {
        return reply(StatusCode::NOT_FOUND, INVALID_INVITATION);
    }
    let username = body.username.trim();
    if username.is_empty() || username.len() > 150 {
        return reply(StatusCode::BAD_REQUEST, "ユーザー名を入力してください");
    }
    if body.password.len() < 8 {
        return reply(
            StatusCode::BAD_REQUEST,
            "パスワードは8文字以上で入力してください",
        );
    }
    let password_hash = match auth_service::hash_password(&body.password) {
        Ok(h) => h,
        Err(e) => return server_error(e),
    };
    let input = AcceptInput {
        username,
        password_hash: &password_hash,
        display_name: body.display_name.trim(),
    };
    let user_id =
        match invitation_repo::accept(&state.pool, &invitation_repo::hash_token(&token), &input)
            .await
        {
            Ok(AcceptOutcome::Created { user_id }) => user_id,
            Ok(AcceptOutcome::Invalid) => return reply(StatusCode::NOT_FOUND, INVALID_INVITATION),
            Ok(AcceptOutcome::EmailTaken) => {
                return reply(
                    StatusCode::CONFLICT,
                    "このメールアドレスは、すでに登録されています。ログインしてください",
                )
            }
            Ok(AcceptOutcome::UsernameTaken) => {
                return reply(StatusCode::CONFLICT, "このユーザー名は既に使用されています")
            }
            Err(e) => return server_error(e),
        };
    match jwt_service::issue_token_pair(
        user_id,
        &state.config.jwt_secret,
        state.config.access_token_lifetime_minutes,
        state.config.refresh_token_lifetime_days,
    ) {
        Ok(pair) => (
            StatusCode::CREATED,
            Json(json!({ "tokens": { "access": pair.access, "refresh": pair.refresh } })),
        )
            .into_response(),
        Err(e) => server_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailIn {
    pub token: String,
}

/// POST /api/v1/auth/verify-email/(公開)
pub async fn verify_email(
    State(state): State<AppState>,
    Json(body): Json<VerifyEmailIn>,
) -> Response {
    let token = body.token.trim();
    if token.len() != 64 {
        return reply(
            StatusCode::BAD_REQUEST,
            "確認のリンクが正しくないか、期限が切れています",
        );
    }
    match invitation_repo::verify_email(&state.pool, &invitation_repo::hash_token(token)).await {
        Ok(Some(_)) => (
            StatusCode::OK,
            Json(json!({ "verified": true, "detail": "メールアドレスを確認しました。ログインしてください" })),
        )
            .into_response(),
        Ok(None) => reply(
            StatusCode::BAD_REQUEST,
            "確認のリンクが正しくないか、期限が切れています",
        ),
        Err(e) => server_error(e),
    }
}

// =============================================================================
// A-3・A-5: 自己登録の方式(`SENN_REGISTRATION_MODE`)
// =============================================================================

/// 自己登録の方式。`restricted`(既定。社内ドメインだけ、メール確認つき)/ `open`(誰でも登録してすぐ使える)。
///
/// 既定は閉じる側(DEMO-000085)。以前は既定が `open` で、本番の設定に何も入れないと外部から登録できた
/// (DEMO-000169)。`open` は、`SENN_REGISTRATION_MODE=open` と明示したときだけ。知らない値は閉じる側。
/// ただし、ユーザーが 1 人もいないときの最初の登録は通す(`register_as_open`。OSS 版の初回の立ち上げ)
pub fn registration_restricted() -> bool {
    registration_mode_is_restricted(std::env::var("SENN_REGISTRATION_MODE").ok().as_deref())
}

fn registration_mode_is_restricted(value: Option<&str>) -> bool {
    !value.is_some_and(|v| v.trim().eq_ignore_ascii_case("open"))
}

/// 自己登録を、制限なし(`open`)の手順で通すか。制限つきでも、ユーザーが 1 人もいなければ最初の 1 人は通す
/// (最初の管理者を作る手段が無いと、OSS 版を入れた直後に誰も入れないため)
pub fn register_as_open(restricted: bool, has_any_user: bool) -> bool {
    !restricted || !has_any_user
}

/// 社内のメールのドメイン(`SENN_INTERNAL_EMAIL_DOMAINS`)に入っているか
pub fn is_internal_email(email: &str) -> bool {
    let Some((_, domain)) = email.trim().rsplit_once('@') else {
        return false;
    };
    let domain = domain.to_ascii_lowercase();
    std::env::var("SENN_INTERNAL_EMAIL_DOMAINS")
        .unwrap_or_default()
        .split(',')
        .map(|d| d.trim().to_ascii_lowercase())
        .any(|d| !d.is_empty() && d == domain)
}

/// 制限つきの自己登録(社内ドメインだけ。確認のメールを送り、確認が済むまで無効)。
/// 応答: 201(確認待ち。トークンは返さない)/ 403(社外。招待を案内)
pub async fn register_restricted(
    state: &AppState,
    headers: &HeaderMap,
    username: &str,
    email: &str,
    password_hash: &str,
    first_name: &str,
    last_name: &str,
) -> Response {
    if !is_internal_email(email) {
        return reply(
            StatusCode::FORBIDDEN,
            "社外のメールアドレスでは登録できません。チームの管理者に招待を依頼してください",
        );
    }
    let user_id = match invitation_repo::create_unverified_user(
        &state.pool,
        username,
        email,
        password_hash,
        first_name,
        last_name,
    )
    .await
    {
        Ok(id) => id,
        Err(e) if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("23505") => {
            return reply(
                StatusCode::BAD_REQUEST,
                "このユーザー名は既に使用されています",
            )
        }
        Err(e) => return server_error(e),
    };
    let token = invitation_repo::new_token();
    if let Err(e) = invitation_repo::create_email_verification(
        &state.pool,
        user_id,
        &invitation_repo::hash_token(&token),
        Utc::now() + Duration::hours(24),
    )
    .await
    {
        return server_error(e);
    }
    let path = format!("/verify-email?token={token}");
    let url = format!("{}{}", mail_link_base(headers, &state.config), path);
    let mail = format!(
        "SENN のアカウント登録を受け付けました。\n\n\
         以下のリンクから24時間以内にメールアドレスを確認してください。確認が済むとログインできます。\n\n\
         {url}\n\n\
         心当たりがない場合は、このメールを無視してください。\n"
    );
    let sent = send_mail(state, email, "SENN メールアドレスの確認", &mail).await;
    let verify_url = (!sent && !is_production()).then_some(path);
    (
        StatusCode::CREATED,
        Json(json!({
            "verificationRequired": true,
            "detail": "確認のメールを送りました。メールのリンクから確認すると、ログインできます",
            "verifyUrl": verify_url,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 登録の方式: 既定(設定なし)と知らない値は閉じる側。`open` と明示したときだけ開く(DEMO-000085)
    #[test]
    fn registration_mode_defaults_to_restricted() {
        assert!(registration_mode_is_restricted(None));
        assert!(registration_mode_is_restricted(Some("")));
        assert!(registration_mode_is_restricted(Some("restricted")));
        assert!(registration_mode_is_restricted(Some("opne")));
        assert!(!registration_mode_is_restricted(Some("open")));
        assert!(!registration_mode_is_restricted(Some(" OPEN ")));
    }

    /// 制限つきでも、ユーザーが 1 人もいないときの最初の登録だけは通す
    #[test]
    fn first_user_can_register_even_when_restricted() {
        assert!(register_as_open(true, false), "最初の 1 人は通す");
        assert!(!register_as_open(true, true), "2 人目からは制限つき");
        assert!(register_as_open(false, true));
    }

    #[test]
    fn internal_email_matches_configured_domains_only() {
        // 環境変数は他のテストと共有するため、このテストの中だけで設定と確認をする
        std::env::set_var("SENN_INTERNAL_EMAIL_DOMAINS", "corp.example, Group.Example");
        assert!(is_internal_email("taro@corp.example"));
        assert!(is_internal_email("hanako@GROUP.example"));
        assert!(!is_internal_email("taro@corp.example.evil.com"));
        assert!(!is_internal_email("taro@outside.example"));
        assert!(!is_internal_email("no-at-mark"));
        std::env::remove_var("SENN_INTERNAL_EMAIL_DOMAINS");
        assert!(
            !is_internal_email("taro@corp.example"),
            "未設定なら社内は無い"
        );
    }
}

/// 招待は Owner とシステム管理者だけ(設計書 §4.3・§9。DEMO-000169)
#[cfg(test)]
mod invite_permission_tests {
    use super::*;
    use crate::test_support::{
        add_test_team_member, create_test_team, create_test_user, test_pool, test_state,
        test_viewer, unique_suffix,
    };

    async fn invite(pool: &sqlx::PgPool, as_user: i32, team: i32, role: &str) -> StatusCode {
        let body: CreateInvitationIn = serde_json::from_value(json!({
            "email": format!("x{}@outside.example", unique_suffix()),
            "role": role,
            "teamId": team,
        }))
        .unwrap();
        create(
            State(test_state(pool).await),
            test_viewer(pool, as_user).await,
            HeaderMap::new(),
            Json(body),
        )
        .await
        .status()
    }

    /// Public(members)チームに Join しただけの人は、社外の人を招待できない
    /// (できると、新規登録を止めていても、正規のメンバーとして人を入れられる)
    #[tokio::test]
    async fn joiner_cannot_invite() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "inv-j").await;
        let owner = create_test_user(&pool, "inv-o").await;
        let joiner = create_test_user(&pool, "inv-j").await;
        add_test_team_member(&pool, team, owner, "admin").await;
        add_test_team_member(&pool, team, joiner, "member").await;

        assert_eq!(
            invite(&pool, joiner, team, "full_member").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            invite(&pool, joiner, team, "guest").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            invite(&pool, owner, team, "guest").await,
            StatusCode::CREATED
        );
    }

    /// 参加しただけの人は、招待の一覧(招待先のメールアドレス)を見られず、Owner の招待を取り消せない
    #[tokio::test]
    async fn joiner_cannot_list_or_revoke_invitations() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "inv-l").await;
        let owner = create_test_user(&pool, "inv-lo").await;
        let joiner = create_test_user(&pool, "inv-lj").await;
        add_test_team_member(&pool, team, owner, "admin").await;
        add_test_team_member(&pool, team, joiner, "member").await;
        assert_eq!(
            invite(&pool, owner, team, "guest").await,
            StatusCode::CREATED
        );
        let invitation_id: i64 = sqlx::query_scalar(
            "SELECT id FROM access_invitation WHERE team_id = $1::int8 ORDER BY id DESC LIMIT 1",
        )
        .bind(team as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        let list_as = |user: i32| {
            let pool = pool.clone();
            async move {
                list(
                    State(test_state(&pool).await),
                    test_viewer(&pool, user).await,
                    Query(ListQuery {
                        team_id: Some(team),
                    }),
                )
                .await
                .status()
            }
        };
        assert_eq!(list_as(joiner).await, StatusCode::FORBIDDEN);
        assert_eq!(list_as(owner).await, StatusCode::OK);

        let status = revoke(
            State(test_state(&pool).await),
            test_viewer(&pool, joiner).await,
            Path(invitation_id.to_string()),
        )
        .await
        .status();
        assert_eq!(status, StatusCode::FORBIDDEN);
        let revoked: bool = sqlx::query_scalar(
            "SELECT revoked_at IS NOT NULL FROM access_invitation WHERE id = $1",
        )
        .bind(invitation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!revoked, "Owner の招待は残っている");
    }
}
