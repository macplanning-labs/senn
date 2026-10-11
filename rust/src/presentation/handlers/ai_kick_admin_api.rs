/// presentation/handlers/ai_kick_admin_api.rs — 「AIに対応させる」の管理者向け API
///
/// 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.6。
/// - `GET/PUT /api/v1/system-admin/ai-kick/`: 有効・緊急停止・上限の設定と、いまの使用状況。
///   **上限を保存していないと、有効にできない**(予算を決めずに有効にできない。「無制限」は存在しない)。
///
/// - `POST /api/v1/system-admin/ai-kick/runners/`: ランナーの登録。**トークンと署名用の秘密は、この 1 回だけ表示する。**
/// - `DELETE /api/v1/system-admin/ai-kick/runners/{id}/`: ランナーの失効(実行中の依頼には停止を求める)。
///
/// すべて管理者のみ。設定の変更と、ランナーの登録・失効は、監査ログに残す(秘密は残さない)。
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::domain::access::Viewer;
use crate::domain::services::ai_kick_policy::Limits;
use crate::infrastructure::encrypted_settings;
use crate::infrastructure::repositories::ai_kick_repo;
use crate::presentation::handlers::system_admin_api::require_staff;
use crate::presentation::state::AppState;

fn detail(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "detail": msg }))).into_response()
}

fn server_error(e: impl std::fmt::Debug) -> Response {
    tracing::error!("[AI自動実行/管理] 処理=API 結果=失敗 | {:?}", e);
    detail(
        StatusCode::INTERNAL_SERVER_ERROR,
        "サーバーエラーが発生しました",
    )
}

#[allow(clippy::result_large_err)] // 呼び出し側でそのまま応答として返すため、Response を持つ
fn staff_only(state: &AppState, viewer: &Viewer) -> Result<i32, Response> {
    require_staff(state, viewer).map_err(|(s, b)| (s, b).into_response())?;
    if viewer.principal.is_key() {
        return Err(detail(
            StatusCode::FORBIDDEN,
            "ログインした管理者だけが変更できます",
        ));
    }
    viewer.require_user_id()
}

/// ランナーの名前: 英数字・ハイフン・アンダースコア・ピリオド、1〜80 字。
fn valid_runner_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 80
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// ランナーの「接続中」の判定(最後の接続からの秒数)。
const ONLINE_WITHIN_SECS: i64 = 120;

async fn view(state: &AppState) -> Result<Value, Response> {
    let settings = ai_kick_repo::get_settings(&state.pool)
        .await
        .map_err(server_error)?;
    let runners = ai_kick_repo::list_runners(&state.pool)
        .await
        .map_err(server_error)?;
    let mut conn = state.pool.acquire().await.map_err(server_error)?;
    let day = ai_kick_repo::budget_used_system(&mut conn)
        .await
        .map_err(server_error)?;
    let week = ai_kick_repo::budget_used_system_week(&mut conn)
        .await
        .map_err(server_error)?;
    drop(conn);
    let now = chrono::Utc::now();
    let pct = |used: i64, budget: Option<u64>| -> Value {
        match budget {
            Some(b) if b > 0 => json!((used.max(0) as u64).saturating_mul(100) / b),
            _ => Value::Null,
        }
    };
    let runners_json: Vec<Value> = runners
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "name": r.name,
                "tokenPrefix": r.token_prefix,
                "createdAt": r.created_at,
                "revokedAt": r.revoked_at,
                "lastSeenAt": r.last_seen_at,
                "online": r.revoked_at.is_none()
                    && r.last_seen_at.map(|t| (now - t).num_seconds() <= ONLINE_WITHIN_SECS).unwrap_or(false),
            })
        })
        .collect();
    Ok(json!({
        "enabled": settings.enabled,
        "killed": settings.killed,
        "limits": settings.limits,
        "allowedRepos": settings.allowed_repos,
        // 画面の初期値の案(保存するまで有効にならない)
        "defaultLimits": Limits::default(),
        "runners": runners_json,
        "usage": {
            "dayUsed": day,
            "weekUsed": week,
            "dayBudget": settings.limits.as_ref().map(|l| l.daily_budget_system),
            "weekBudget": settings.limits.as_ref().map(|l| l.weekly_budget_system),
            "dayPercent": pct(day, settings.limits.as_ref().map(|l| l.daily_budget_system)),
            "weekPercent": pct(week, settings.limits.as_ref().map(|l| l.weekly_budget_system)),
        },
    }))
}

/// GET /api/v1/system-admin/ai-kick/
pub async fn get_settings(State(state): State<AppState>, viewer: Viewer) -> Response {
    if let Err(r) = staff_only(&state, &viewer) {
        return r;
    }
    match view(&state).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(r) => r,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBody {
    pub enabled: Option<bool>,
    pub killed: Option<bool>,
    pub limits: Option<Limits>,
    /// AI が作業してよいリポジトリ(owner/name)。送ったときだけ置き換える
    pub allowed_repos: Option<Vec<String>>,
}

/// 許可するリポジトリの一覧を検証して、前後の空白を落とし・重複を除く。
fn normalize_allowed_repos(repos: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for r in repos {
        let r = r.trim();
        if r.is_empty() {
            continue;
        }
        if !crate::presentation::handlers::ai_kick_api::valid_repo(r) {
            return Err(format!(
                "リポジトリは「owner/name」の形式で指定してください: {r}"
            ));
        }
        if !out.iter().any(|x| x.eq_ignore_ascii_case(r)) {
            out.push(r.to_string());
        }
    }
    if out.len() > 200 {
        return Err("許可するリポジトリは 200 件までです".to_string());
    }
    Ok(out)
}

/// PUT /api/v1/system-admin/ai-kick/
pub async fn put_settings(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<SettingsBody>,
) -> Response {
    let admin_id = match staff_only(&state, &viewer) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let current = match ai_kick_repo::get_settings(&state.pool).await {
        Ok(s) => s,
        Err(e) => return server_error(e),
    };
    // 上限の保存(検証してから)
    if let Some(l) = &body.limits {
        if let Err(msg) = l.validate() {
            return detail(StatusCode::BAD_REQUEST, &msg);
        }
        if let Err(e) = ai_kick_repo::set_limits(&state.pool, l).await {
            return server_error(e);
        }
        ai_kick_repo::audit(
            &state.pool,
            Some(i64::from(admin_id)),
            "human",
            "ai_kick.limits_changed",
            json!({ "limits": l }),
        )
        .await;
    }
    // AI が作業してよいリポジトリ
    if let Some(repos) = &body.allowed_repos {
        let repos = match normalize_allowed_repos(repos) {
            Ok(r) => r,
            Err(msg) => return detail(StatusCode::BAD_REQUEST, &msg),
        };
        if let Err(e) = ai_kick_repo::set_allowed_repos(&state.pool, &repos).await {
            return server_error(e);
        }
        ai_kick_repo::audit(
            &state.pool,
            Some(i64::from(admin_id)),
            "human",
            "ai_kick.allowed_repos_changed",
            json!({ "allowedRepos": repos }),
        )
        .await;
    }
    // 有効にするには、上限が保存されていること
    if let Some(enabled) = body.enabled {
        let has_limits = body.limits.is_some() || current.limits.is_some();
        if enabled && !has_limits {
            return detail(
                StatusCode::BAD_REQUEST,
                "上限(予算など)を保存してから、有効にしてください(無制限では有効にできません)",
            );
        }
        if let Err(e) = ai_kick_repo::set_enabled(&state.pool, enabled).await {
            return server_error(e);
        }
        ai_kick_repo::audit(
            &state.pool,
            Some(i64::from(admin_id)),
            "human",
            if enabled {
                "ai_kick.enabled"
            } else {
                "ai_kick.disabled"
            },
            json!({}),
        )
        .await;
    }
    // 緊急停止の作動・解除
    if let Some(killed) = body.killed {
        if let Err(e) = ai_kick_repo::set_killed(&state.pool, killed).await {
            return server_error(e);
        }
        ai_kick_repo::audit(
            &state.pool,
            Some(i64::from(admin_id)),
            "human",
            if killed {
                "ai_kick.killed"
            } else {
                "ai_kick.unkilled"
            },
            json!({}),
        )
        .await;
    }
    match view(&state).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(r) => r,
    }
}

#[derive(Deserialize)]
pub struct CreateRunnerBody {
    pub name: String,
}

/// POST /api/v1/system-admin/ai-kick/runners/ — トークンと署名用の秘密は、この応答だけに含まれる。
pub async fn create_runner(
    State(state): State<AppState>,
    viewer: Viewer,
    Json(body): Json<CreateRunnerBody>,
) -> Response {
    let admin_id = match staff_only(&state, &viewer) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let name = body.name.trim().to_string();
    if !valid_runner_name(&name) {
        return detail(
            StatusCode::BAD_REQUEST,
            "ランナーの名前は、英数字・ハイフン・アンダースコア・ピリオドで 1〜80 字にしてください",
        );
    }
    // 乱数の生成器は、await をまたいで持たない(Send でないため)
    let (t, sgn) = {
        let mut rng = rand::thread_rng();
        let mut t = [0u8; 32];
        let mut sgn = [0u8; 32];
        rng.fill_bytes(&mut t);
        rng.fill_bytes(&mut sgn);
        (t, sgn)
    };
    let token = format!("srun_{}", URL_SAFE_NO_PAD.encode(t));
    let signing_secret = URL_SAFE_NO_PAD.encode(sgn);
    let enc = match encrypted_settings::encrypt_value(&signing_secret, &state.config.jwt_secret) {
        Ok(e) => e,
        Err(e) => return server_error(e),
    };
    let prefix: String = token.chars().take(12).collect();
    let runner = match ai_kick_repo::create_runner(
        &state.pool,
        &name,
        &ai_kick_repo::hash_token(&token),
        &prefix,
        &enc,
        i64::from(admin_id),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            // 名前の重複(UNIQUE 制約)は、利用者に分かる形で返す
            if e.to_string().contains("t_ai_runner_name_key") {
                return detail(StatusCode::CONFLICT, "同じ名前のランナーがすでにあります");
            }
            return server_error(e);
        }
    };
    ai_kick_repo::audit(
        &state.pool,
        Some(i64::from(admin_id)),
        "human",
        "ai_kick.runner_created",
        json!({ "runnerId": runner.id, "name": runner.name }),
    )
    .await;
    (
        StatusCode::CREATED,
        Json(json!({
            "id": runner.id,
            "name": runner.name,
            "token": token,
            "signingSecret": signing_secret,
            "notice": "トークンと署名用の秘密は、この 1 回だけ表示されます。ランナーの設定に貼り付けて、安全な場所に保管してください",
        })),
    )
        .into_response()
}

/// DELETE /api/v1/system-admin/ai-kick/runners/{id}/
pub async fn revoke_runner(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Response {
    let admin_id = match staff_only(&state, &viewer) {
        Ok(i) => i,
        Err(r) => return r,
    };
    match ai_kick_repo::revoke_runner(&state.pool, id).await {
        Ok(true) => {
            ai_kick_repo::audit(
                &state.pool,
                Some(i64::from(admin_id)),
                "human",
                "ai_kick.runner_revoked",
                json!({ "runnerId": id }),
            )
            .await;
            (StatusCode::OK, Json(json!({ "revoked": true }))).into_response()
        }
        Ok(false) => detail(
            StatusCode::NOT_FOUND,
            "ランナーが見つからない、またはすでに失効しています",
        ),
        Err(e) => server_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_names_are_validated() {
        assert!(valid_runner_name("mac-mini-1"));
        assert!(valid_runner_name("runner_01.prod"));
        for bad in ["", "a b", "a/b", "名前", "a;b", &"x".repeat(81)] {
            assert!(!valid_runner_name(bad), "拒否される: {bad:?}");
        }
    }
}
