/// presentation/handlers/git_status_rule_api.rs — Git ステータス自動更新ルール API
///
/// 設計: docs/design/詳細設計_GitHub連携Phase2.md §3
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::access::Viewer;
use crate::domain::services::git_webhook_service::{self, GitTrigger, VALID_TARGET_CATEGORIES};
use crate::infrastructure::repositories::{git_status_rule_repo, workflow_status_repo};
use crate::presentation::state::AppState;

fn err(detail: &str) -> serde_json::Value {
    json!({"detail": detail})
}

// ── Request/Response types ──

#[derive(Debug, Deserialize)]
pub struct GetRulesQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
pub struct RuleOut {
    pub trigger: String,
    pub action: String,
    #[serde(rename = "targetSlug")]
    pub target_slug: Option<String>,
    #[serde(rename = "targetCategory")]
    pub target_category: Option<String>,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
pub struct StatusOut {
    pub slug: String,
    pub name: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
pub struct GetRulesOut {
    pub rules: Vec<RuleOut>,
    pub statuses: Vec<StatusOut>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RuleIn {
    pub trigger: String,
    pub action: String,
    #[serde(rename = "targetSlug")]
    pub target_slug: Option<String>,
    #[serde(rename = "targetCategory")]
    pub target_category: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PutRulesIn {
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub rules: Vec<RuleIn>,
}

/// GET /api/v1/git-status-rules/?project=<id> または ?team=<id>
pub async fn get_rules(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(params): Query<GetRulesQuery>,
) -> impl IntoResponse {
    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        params.project,
        params.team,
        "GET /api/v1/git-status-rules/",
    )
    .await
    {
        return r;
    }

    match load_view(&state, params.project, params.team).await {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(r) => r,
    }
}

/// 保存されている規則と標準を合わせた、画面向けの応答を作る(GET と PUT の保存後の応答で共通)。
async fn load_view(
    state: &AppState,
    project: Option<i32>,
    team: Option<i32>,
) -> Result<GetRulesOut, axum::response::Response> {
    let server_error = || {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response()
    };
    let statuses = workflow_status_repo::list_statuses_for_scope(&state.pool, project, team)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list statuses: {:?}", e);
            server_error()
        })?
        .into_iter()
        .map(|(slug, name, category)| StatusOut {
            slug,
            name,
            category,
        })
        .collect();
    let stored_rules = git_status_rule_repo::list_for_scope(&state.pool, project, team)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list rules: {:?}", e);
            server_error()
        })?;
    // すべてのトリガーに対する出力。保存が無いものは標準の内容で isDefault: true
    let rules: Vec<RuleOut> = GitTrigger::ALL
        .iter()
        .map(|trigger| {
            let trigger_str = trigger.as_str();
            if let Some(stored) = stored_rules.iter().find(|r| r.trigger == trigger_str) {
                RuleOut {
                    trigger: trigger_str.to_string(),
                    action: stored.action.clone(),
                    target_slug: stored.target_slug.clone(),
                    target_category: stored.target_category.clone(),
                    is_default: false,
                }
            } else {
                let (action, category) = git_webhook_service::builtin_as_stored(*trigger);
                RuleOut {
                    trigger: trigger_str.to_string(),
                    action: action.to_string(),
                    target_slug: None,
                    target_category: category.map(|c| c.to_string()),
                    is_default: true,
                }
            }
        })
        .collect();
    Ok(GetRulesOut { rules, statuses })
}

/// PUT /api/v1/git-status-rules/
pub async fn put_rules(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<PutRulesIn>,
) -> impl IntoResponse {
    // project/team のどちらか一方だけを指定
    if (body.project.is_some() && body.team.is_some())
        || (body.project.is_none() && body.team.is_none())
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(err("project か team のどちらか一方を指定してください")),
        )
            .into_response();
    }

    if let Err(r) = crate::presentation::handlers::access_guard::require_scope_manager_v(
        &state.pool,
        &viewer,
        body.project,
        body.team,
        "PUT /api/v1/git-status-rules/",
    )
    .await
    {
        return r;
    }

    // ルールの検証
    let mut seen_triggers = std::collections::HashSet::new();
    for rule in &body.rules {
        // trigger のパース
        if GitTrigger::parse(&rule.trigger).is_none() {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("未知のトリガーが含まれています")),
            )
                .into_response();
        }

        // 同じトリガーの重複
        if !seen_triggers.insert(&rule.trigger) {
            return (
                StatusCode::BAD_REQUEST,
                Json(err("同じトリガーの規則が複数含まれています")),
            )
                .into_response();
        }

        // action の検証
        if rule.action != "set" && rule.action != "none" {
            return (
                StatusCode::BAD_REQUEST,
                Json(err(
                    "action は \"set\" または \"none\" である必要があります",
                )),
            )
                .into_response();
        }

        // "none" のとき slug と category は無し
        if rule.action == "none" {
            if rule.target_slug.is_some() || rule.target_category.is_some() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(err(
                        "action が \"none\" のとき、targetSlug と targetCategory は指定できません",
                    )),
                )
                    .into_response();
            }
        }

        // "set" のとき slug と category のどちらか一方だけ
        if rule.action == "set" {
            let has_slug = rule.target_slug.is_some();
            let has_category = rule.target_category.is_some();
            if has_slug == has_category {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(err("action が \"set\" のとき、targetSlug と targetCategory のどちらか一方だけを指定してください")),
                )
                    .into_response();
            }

            // targetCategory の検証
            if let Some(ref cat) = rule.target_category {
                if !VALID_TARGET_CATEGORIES.contains(&cat.as_str()) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(err("targetCategory が無効です")),
                    )
                        .into_response();
                }
            }

            // targetSlug の存在確認
            if let Some(ref slug) = rule.target_slug {
                match workflow_status_repo::status_slug_exists_for_scope(
                    &state.pool,
                    body.project,
                    body.team,
                    slug,
                )
                .await
                {
                    Ok(exists) => {
                        if !exists {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(err("targetSlug がこのスコープに存在しません")),
                            )
                                .into_response();
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to check slug existence: {:?}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(err("サーバーエラーが発生しました")),
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    // StoredRule に変換して保存
    let stored_rules: Vec<git_status_rule_repo::StoredRule> = body
        .rules
        .into_iter()
        .map(|rule| git_status_rule_repo::StoredRule {
            trigger: rule.trigger,
            action: rule.action,
            target_slug: rule.target_slug,
            target_category: rule.target_category,
        })
        .collect();

    if let Err(e) = git_status_rule_repo::replace_for_scope(
        &state.pool,
        body.project,
        body.team,
        &stored_rules,
        user_id,
    )
    .await
    {
        tracing::error!("Failed to replace rules: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err("サーバーエラーが発生しました")),
        )
            .into_response();
    }

    // 保存後の状態を返す
    match load_view(&state, body.project, body.team).await {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(r) => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        add_test_team_member, create_test_project, create_test_user, test_pool, test_state,
        test_viewer,
    };

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap_or(json!(null))
    }

    fn rule(trigger: &str, action: &str, slug: Option<&str>, cat: Option<&str>) -> RuleIn {
        RuleIn {
            trigger: trigger.into(),
            action: action.into(),
            target_slug: slug.map(String::from),
            target_category: cat.map(String::from),
        }
    }

    async fn put(
        state: &AppState,
        viewer: Viewer,
        project: Option<i32>,
        team: Option<i32>,
        rules: Vec<RuleIn>,
    ) -> axum::response::Response {
        put_rules(
            State(state.clone()),
            viewer,
            Json(PutRulesIn {
                project,
                team,
                rules,
            }),
        )
        .await
        .into_response()
    }

    #[tokio::test]
    async fn rules_api_get_put_validation_and_permissions() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let state = test_state(&pool).await;
        let admin = create_test_user(&pool, "rule_admin").await;
        let outsider = create_test_user(&pool, "rule_outsider").await;
        let project = create_test_project(&pool, "RAP", admin).await;
        let team: i32 = sqlx::query_scalar(
            "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
        )
        .bind(project)
        .fetch_one(&pool)
        .await
        .unwrap();
        add_test_team_member(&pool, team, admin, "admin").await;
        for (slug, cat, pos) in [("todo", "unstarted", 0), ("doing", "started", 10)] {
            sqlx::query(
                "INSERT INTO t_workflow_status (project_id, slug, category, name, color, position, is_default)
                 VALUES ($1, $2, $3, $2, '#888888', $4, false)",
            )
            .bind(project)
            .bind(slug)
            .bind(cat)
            .bind(pos)
            .execute(&pool)
            .await
            .unwrap();
        }

        // 1. GET: 9 トリガーすべてが標準(isDefault: true)で、そのスコープのステータスが選択肢になる
        let r = get_rules(
            State(state.clone()),
            test_viewer(&pool, admin).await,
            Query(GetRulesQuery {
                project: Some(project),
                team: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(r.status(), StatusCode::OK);
        let v = body_json(r).await;
        assert_eq!(v["rules"].as_array().unwrap().len(), 9);
        assert!(v["rules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["isDefault"] == true));
        let merged = v["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["trigger"] == "pr_merged")
            .unwrap();
        assert_eq!(
            (merged["action"].as_str(), merged["targetCategory"].as_str()),
            (Some("set"), Some("completed"))
        );
        let draft = v["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["trigger"] == "pr_draft")
            .unwrap();
        assert_eq!(draft["action"], "none");
        let slugs: Vec<&str> = v["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s["slug"].as_str())
            .collect();
        assert_eq!(slugs, vec!["todo", "doing"]);

        // 2. PUT: 変えたものだけ保存され、GET に反映される。渡さなかったものは標準のまま
        let r = put(
            &state,
            test_viewer(&pool, admin).await,
            Some(project),
            None,
            vec![
                rule("pr_draft", "set", Some("doing"), None),
                rule("pr_merged", "none", None, None),
                rule("pr_closed", "set", None, Some("unstarted")),
            ],
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
        let v = body_json(r).await;
        let by = |t: &str| {
            v["rules"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["trigger"] == t)
                .unwrap()
                .clone()
        };
        assert_eq!(
            (
                by("pr_draft")["isDefault"].clone(),
                by("pr_draft")["targetSlug"].clone()
            ),
            (json!(false), json!("doing"))
        );
        assert_eq!(
            (
                by("pr_merged")["isDefault"].clone(),
                by("pr_merged")["action"].clone()
            ),
            (json!(false), json!("none"))
        );
        assert_eq!(by("pr_closed")["targetCategory"], "unstarted");
        assert_eq!(by("branch_pushed")["isDefault"], true);

        // 3. 空で PUT すると、すべて標準に戻る
        let r = put(
            &state,
            test_viewer(&pool, admin).await,
            Some(project),
            None,
            vec![],
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
        assert!(body_json(r).await["rules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["isDefault"] == true));

        // 4. 検証エラー(400)。保存は変わらない
        let bad: Vec<(&str, Vec<RuleIn>, Option<i32>, Option<i32>)> = vec![
            (
                "未知のトリガー",
                vec![rule("nope", "none", None, None)],
                Some(project),
                None,
            ),
            (
                "流れに無い slug",
                vec![rule("pr_draft", "set", Some("nope"), None)],
                Some(project),
                None,
            ),
            (
                "重複",
                vec![
                    rule("pr_draft", "none", None, None),
                    rule("pr_draft", "none", None, None),
                ],
                Some(project),
                None,
            ),
            ("project と team の両方", vec![], Some(project), Some(team)),
            ("どちらも無し", vec![], None, None),
            (
                "set で slug と category の両方",
                vec![rule("pr_draft", "set", Some("doing"), Some("started"))],
                Some(project),
                None,
            ),
            (
                "set でどちらも無し",
                vec![rule("pr_draft", "set", None, None)],
                Some(project),
                None,
            ),
            (
                "none に slug",
                vec![rule("pr_draft", "none", Some("doing"), None)],
                Some(project),
                None,
            ),
            (
                "無効なカテゴリ",
                vec![rule("pr_draft", "set", None, Some("whatever"))],
                Some(project),
                None,
            ),
            (
                "無効な action",
                vec![rule("pr_draft", "maybe", None, None)],
                Some(project),
                None,
            ),
        ];
        for (name, rules, p, t) in bad {
            let r = put(&state, test_viewer(&pool, admin).await, p, t, rules).await;
            assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{name}");
        }
        let n: i64 =
            sqlx::query_scalar("SELECT count(*) FROM t_git_status_rule WHERE project_id = $1")
                .bind(project as i64)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 0, "検証エラーでは何も保存しない");

        // 5. 管理者でない人は、取得も保存もできない(403)
        let r = get_rules(
            State(state.clone()),
            test_viewer(&pool, outsider).await,
            Query(GetRulesQuery {
                project: Some(project),
                team: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        let r = put(
            &state,
            test_viewer(&pool, outsider).await,
            Some(project),
            None,
            vec![rule("pr_draft", "none", None, None)],
        )
        .await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }
}
