/// presentation/handlers/github_app_link_api.rs — GitHub App 連携状態・リポジトリ・リンク API
///
/// 設計: docs/design/詳細設計_GitHub連携かんたん接続.md §3.3
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::access::Viewer;
use crate::infrastructure::github_app;
use crate::infrastructure::repositories::integration_repo;
use crate::presentation::state::AppState;

fn err(detail: &str) -> serde_json::Value {
    json!({"detail": detail})
}

// ── Status API ──

#[derive(Debug, Serialize)]
#[serde(crate = "serde")]
pub struct AppInfo {
    pub slug: String,
    pub name: String,
    #[serde(rename = "htmlUrl")]
    pub html_url: String,
}

#[derive(Debug, Serialize)]
#[serde(crate = "serde")]
pub struct InstallationStatus {
    #[serde(rename = "installationId")]
    pub installation_id: i64,
    #[serde(rename = "accountLogin")]
    pub account_login: String,
    #[serde(rename = "accountType")]
    pub account_type: String,
    #[serde(rename = "repositorySelection")]
    pub repository_selection: String,
    pub suspended: bool,
}

#[derive(Debug, Serialize)]
#[serde(crate = "serde")]
pub struct GitHubAppStatusOut {
    pub configured: bool,
    #[serde(rename = "canCreateApp")]
    pub can_create_app: bool,
    pub app: Option<AppInfo>,
    pub installations: Vec<InstallationStatus>,
    /// 閲覧者が、問い合わせたチームのメンバーか。チームを指定しなかったとき・プロジェクト単位のときは null。
    /// メンバーではないシステム管理者が連携を作ると、操作の記録が管理者の名前になるため、画面で案内を出す。
    #[serde(rename = "viewerIsMember")]
    pub viewer_is_member: Option<bool>,
    /// GitHub App を作れない理由(この環境のアドレスが、インターネットから届かないなど)。作れるなら null。
    /// GitHub が作成画面を出す前に英語のエラーで断る前に、SENN の画面で案内するために返す。
    #[serde(rename = "createBlockedReason")]
    pub create_blocked_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StatusQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
}

/// 閲覧者が、そのチームのメンバーか。チームを指定しなかったとき、またはプロジェクト単位の問い合わせは None(案内は出さない)。
/// プロジェクトは複数のチームにまたがり、所属の判定が複雑なため、誤った警告を出さないよう今は判定しない。
fn viewer_is_member_of(viewer: &Viewer, q: &StatusQuery) -> Option<bool> {
    if q.project.is_some() {
        return None;
    }
    q.team
        .map(|team_id| viewer.team_membership(team_id).is_some())
}

/// GET /api/v1/github-app/status/
pub async fn status(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(q): Query<StatusQuery>,
) -> impl IntoResponse {
    // ログイン済みなら誰でも
    let _user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match github_app::store::app_exists(&state.pool).await {
        Ok(app_exists) => {
            let app = if app_exists {
                match github_app::store::get_app_summary(&state.pool).await {
                    Ok(Some(app)) => Some(AppInfo {
                        slug: app.slug,
                        name: app.name,
                        html_url: app.html_url,
                    }),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::error!("Failed to get app summary: {:?}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(err("サーバーエラーが発生しました")),
                        )
                            .into_response();
                    }
                }
            } else {
                None
            };

            let installations = match github_app::store::list_installations(&state.pool).await {
                Ok(rows) => rows
                    .into_iter()
                    .map(|r| InstallationStatus {
                        installation_id: r.installation_id,
                        account_login: r.account_login,
                        account_type: r.account_type,
                        repository_selection: r.repository_selection,
                        suspended: r.suspended,
                    })
                    .collect(),
                Err(e) => {
                    tracing::error!("Failed to list installations: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(err("サーバーエラーが発生しました")),
                    )
                        .into_response();
                }
            };

            let can_create_app = !app_exists && viewer.is_system_admin();

            let response = GitHubAppStatusOut {
                configured: app_exists,
                can_create_app,
                app,
                installations,
                viewer_is_member: viewer_is_member_of(&viewer, &q),
                create_blocked_reason: github_app::manifest::unreachable_webhook_reason(
                    &github_app::manifest::webhook_url(&state.config.base_url),
                    &github_app::client::web_base(),
                ),
            };

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to check app existence: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response()
        }
    }
}

// ── Repositories API ──

#[derive(Debug, Deserialize)]
pub struct RepositoriesQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(crate = "serde")]
pub struct GitHubRepoOut {
    #[serde(rename = "installationId")]
    pub installation_id: i64,
    #[serde(rename = "fullName")]
    pub full_name: String,
    #[serde(rename = "htmlUrl")]
    pub html_url: String,
    pub private: bool,
    pub linked: bool,
}

/// GET /api/v1/github-app/repositories/?project=&team=
pub async fn repositories(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<RepositoriesQuery>,
) -> impl IntoResponse {
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        params.project,
        params.team,
        "GET /api/v1/github-app/repositories/",
    )
    .await
    {
        return r;
    }

    // App 未作成なら 409
    match github_app::store::app_exists(&state.pool).await {
        Ok(false) => {
            return (
                StatusCode::CONFLICT,
                Json(err("GitHub連携がまだ有効になっていません")),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to check app existence: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
        _ => {}
    }

    // App の認証情報と、連携済みのリポジトリ(どのプロジェクト/チームのものも含む)を取得
    let (app_creds, existing_app_repos) = match tokio::join!(
        github_app::store::get_app_credentials(&state.pool, &state.config.jwt_secret),
        github_app::store::linked_repo_urls(&state.pool)
    ) {
        (Ok(Some(creds)), Ok(linked)) => (creds, linked),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to get app credentials or linked repos: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
        (Ok(None), _) => {
            return (
                StatusCode::CONFLICT,
                Json(err("GitHub連携がまだ有効になっていません")),
            )
                .into_response();
        }
    };

    // 全インストールのリポジトリを集約
    let installations = match github_app::store::list_installations(&state.pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Failed to list installations: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    let mut all_repos = Vec::new();

    for inst in installations {
        let jwt_result = github_app::client::app_jwt(app_creds.0, &app_creds.1);
        let jwt = match jwt_result {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!(
                    "Failed to create JWT for installation {}: {}",
                    inst.installation_id,
                    e
                );
                continue;
            }
        };

        let token_result = github_app::client::installation_token(&jwt, inst.installation_id).await;
        let token = match token_result {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    "Failed to get installation token for {}: {}",
                    inst.installation_id,
                    e
                );
                continue;
            }
        };

        let repos_result = github_app::client::list_repositories(&token).await;
        match repos_result {
            Ok(repos) => {
                for repo in repos {
                    let repo_url = format!("https://github.com/{}", repo.full_name).to_lowercase();
                    let linked = existing_app_repos.contains(&repo_url);

                    all_repos.push(GitHubRepoOut {
                        installation_id: inst.installation_id,
                        full_name: repo.full_name,
                        html_url: repo.html_url,
                        private: repo.private,
                        linked,
                    });
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to list repositories for installation {}: {}",
                    inst.installation_id,
                    e
                );
                continue;
            }
        }
    }

    (StatusCode::OK, Json(all_repos)).into_response()
}

// ── Integrations API ──

#[derive(Debug, Deserialize)]
pub struct CreateIntegrationsIn {
    pub project: Option<i32>,
    pub team: Option<i32>,
    #[serde(rename = "installationId")]
    pub installation_id: i64,
    pub repositories: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(crate = "serde")]
pub struct CreateIntegrationsOut {
    pub created: Vec<String>,
    pub skipped: Vec<String>,
    pub rejected: Vec<String>,
}

/// POST /api/v1/github-app/integrations/
pub async fn create_integrations(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<CreateIntegrationsIn>,
) -> impl IntoResponse {
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        body.project,
        body.team,
        "POST /api/v1/github-app/integrations/",
    )
    .await
    {
        return r;
    }

    if body.project.is_some() && body.team.is_some() {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("project か team のどちらか一方を指定してください")),
        )
            .into_response();
    }

    let created_by = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    // インストールが存在するか確認
    match github_app::store::installation_exists(&state.pool, body.installation_id).await {
        Ok(false) => return (StatusCode::NOT_FOUND, Json(err("見つかりません"))).into_response(),
        Err(e) => {
            tracing::error!("Failed to check installation existence: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
        _ => {}
    }

    // App の認証情報を取得
    let app_creds =
        match github_app::store::get_app_credentials(&state.pool, &state.config.jwt_secret).await {
            Ok(Some(creds)) => creds,
            Ok(None) => {
                return (
                    StatusCode::CONFLICT,
                    Json(err("GitHub連携がまだ有効になっていません")),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Failed to get app credentials: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        };

    // 実際のリポジトリ一覧を取得
    let jwt = match github_app::client::app_jwt(app_creds.0, &app_creds.1) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to create JWT: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    let token = match github_app::client::installation_token(&jwt, body.installation_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to get installation token: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    let real_repos = match github_app::client::list_repositories(&token).await {
        Ok(repos) => repos,
        Err(e) => {
            tracing::error!("Failed to list repositories: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err("サーバーエラーが発生しました")),
            )
                .into_response();
        }
    };

    let real_full_names: std::collections::HashSet<String> =
        real_repos.iter().map(|r| r.full_name.clone()).collect();

    // 入力リポジトリを検証・分類
    let mut created = Vec::new();
    let mut skipped = Vec::new();
    let mut rejected = Vec::new();

    for full_name in body.repositories.iter().take(100) {
        // fullName 形式を検証
        if !github_app::is_valid_full_name(full_name) {
            rejected.push(full_name.clone());
            continue;
        }

        // 実リポジトリ一覧に含まれているか確認
        if !real_full_names.contains(full_name) {
            rejected.push(full_name.clone());
            continue;
        }

        // DB に INSERT
        let repository_url = format!("https://github.com/{}", full_name);

        // 既に連携済みか確認
        let _existing: Option<()> =
            match integration_repo::find_by_url(&state.pool, &repository_url).await {
                Ok(Some(i)) if i.installation_id == Some(body.installation_id) => {
                    skipped.push(full_name.clone());
                    continue;
                }
                Ok(_) => None, // 別のインストール経由で連携済み、またはなし
                Err(e) => {
                    tracing::error!("Failed to check existing integration: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(err("サーバーエラーが発生しました")),
                    )
                        .into_response();
                }
            };

        // INSERT(tricky: can_create_in は project/team のどちらか一方が必要)
        use crate::domain::models::integration_api::GitIntegrationWriteIn;

        let write_in = GitIntegrationWriteIn {
            project: body.project,
            team: body.team,
            provider: "github".to_string(),
            repository_url: repository_url.clone(),
            webhook_secret: String::new(),
            is_active: true,
            auto_status_transition: true,
        };

        // 実際に insert する DB 層を呼び出す
        match insert_github_integration(&state.pool, &write_in, created_by, body.installation_id)
            .await
        {
            Ok(_) => created.push(full_name.clone()),
            Err(e) if e.to_string().contains("unique constraint") => {
                skipped.push(full_name.clone());
            }
            Err(e) => {
                tracing::error!("Failed to insert integration: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err("サーバーエラーが発生しました")),
                )
                    .into_response();
            }
        }
    }

    let response = CreateIntegrationsOut {
        created,
        skipped,
        rejected,
    };

    (StatusCode::CREATED, Json(response)).into_response()
}

/// Helper: GitHub App 経由の統合を INSERT する
async fn insert_github_integration(
    pool: &sqlx::PgPool,
    input: &crate::domain::models::integration_api::GitIntegrationWriteIn,
    created_by: i32,
    installation_id: i64,
) -> anyhow::Result<i32> {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO t_git_integration
            (project_id, team_id, provider, repository_url, webhook_secret, is_active, auto_status_transition, created_by_id, installation_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
         RETURNING id::int4"
    )
    .bind(input.project)
    .bind(input.team)
    .bind(&input.provider)
    .bind(&input.repository_url)
    .bind(&input.webhook_secret)
    .bind(input.is_active)
    .bind(input.auto_status_transition)
    .bind(created_by)
    .bind(installation_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_app_status_camel_case() {
        // Simply verify the struct serializes with correct keys
        let status = GitHubAppStatusOut {
            configured: true,
            can_create_app: false,
            app: Some(AppInfo {
                slug: "test-app".to_string(),
                name: "Test App".to_string(),
                html_url: "https://github.com/apps/test-app".to_string(),
            }),
            installations: vec![],
            viewer_is_member: Some(false),
            create_blocked_reason: Some("理由".to_string()),
        };

        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["viewerIsMember"], false);
        assert_eq!(json["createBlockedReason"], "理由");
        assert!(json["canCreateApp"].is_boolean());
        assert!(json["app"]["htmlUrl"].is_string());
        assert!(json["installations"].is_array());
    }

    fn viewer_of(role: crate::domain::access::Role, team_ids: &[i32]) -> Viewer {
        use crate::domain::access::{Membership, MembershipKind, Principal};
        Viewer {
            principal: Principal::Human { user_id: 1 },
            role,
            memberships: team_ids
                .iter()
                .map(|&team_id| Membership {
                    team_id,
                    kind: MembershipKind::Team,
                    is_owner: false,
                    expires_on: None,
                })
                .collect(),
            public_team_ids: vec![],
        }
    }

    #[test]
    fn viewer_is_member_of_the_team_only_when_in_it() {
        use crate::domain::access::Role;
        let team = |id| StatusQuery {
            project: None,
            team: Some(id),
        };
        let member = viewer_of(Role::FullMember, &[7]);
        assert_eq!(viewer_is_member_of(&member, &team(7)), Some(true));
        assert_eq!(viewer_is_member_of(&member, &team(8)), Some(false));
        // システム管理者でも、チームのメンバーでなければ false(案内を出す)
        let admin = viewer_of(Role::SystemAdmin, &[]);
        assert_eq!(viewer_is_member_of(&admin, &team(7)), Some(false));
    }

    #[test]
    fn viewer_is_member_is_unknown_without_a_team_or_for_projects() {
        use crate::domain::access::Role;
        let member = viewer_of(Role::FullMember, &[7]);
        let none = StatusQuery {
            project: None,
            team: None,
        };
        assert_eq!(viewer_is_member_of(&member, &none), None);
        // プロジェクト単位は判定しない(誤った警告を出さない)
        let project = StatusQuery {
            project: Some(3),
            team: Some(7),
        };
        assert_eq!(viewer_is_member_of(&member, &project), None);
    }

    #[test]
    fn test_repo_out_camel_case() {
        let repo = GitHubRepoOut {
            installation_id: 123,
            full_name: "owner/repo".to_string(),
            html_url: "https://github.com/owner/repo".to_string(),
            private: false,
            linked: true,
        };

        let json = serde_json::to_value(&repo).unwrap();
        assert_eq!(json["installationId"], 123);
        assert_eq!(json["fullName"], "owner/repo");
        assert!(json["htmlUrl"].is_string());
    }
}
