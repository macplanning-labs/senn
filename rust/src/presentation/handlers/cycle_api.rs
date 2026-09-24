/// presentation/handlers/cycle_api.rs — JSON サイクル API
///
/// Djangoの /api/v1/cycles/* (基本CRUD) と挙動を一致させるハンドラー。
/// progress/complete/velocity/burndown は含めない。

use axum::{
    extract::{State, Path, Query},
    response::IntoResponse,
    http::StatusCode,
    Json,
    Extension,
};
use serde::{Deserialize, Serialize};

use crate::presentation::state::AppState;
use crate::presentation::middleware::jwt_auth::AuthUser;
use crate::infrastructure::repositories::cycle_repo;
use crate::domain::models::cycle_api::{CycleWriteIn, CyclePatchIn, CycleGraphPositionIn};

#[derive(Deserialize)]
pub struct ListQuery {
    pub project: Option<i32>,
    pub team: Option<i32>,
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub detail: String,
}

/// GET /api/v1/cycles/?project=<id>&team=<id>&status=<s> — サイクル一覧
pub async fn list(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    match cycle_repo::find_all_cycles(
        &state.pool,
        params.project,
        params.team,
        params.status.as_deref(),
    )
    .await
    {
        Ok(cycles) => (StatusCode::OK, Json(cycles)).into_response(),
        Err(e) => {
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/cycles/{id}/ — サイクル詳細
pub async fn detail(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cycle_repo::find_cycle_by_id(&state.pool, id).await {
        Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/cycles/ — サイクル作成
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(input): Json<CycleWriteIn>,
) -> impl IntoResponse {
    match cycle_repo::create_cycle(&state.pool, &input, auth.user_id).await {
        Ok(id) => {
            // 作成したサイクルを返す
            match cycle_repo::find_cycle_by_id(&state.pool, id).await {
                Ok(Some(cycle)) => (StatusCode::CREATED, Json(cycle)).into_response(),
                Ok(None) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "作成されたサイクルが見つかりません".to_string(),
                    }),
                )
                    .into_response(),
                Err(e) => {
                    tracing::error!("[サイクル/作成検証] 処理=作成後検証 結果=失敗 影響=サイクル登録状態が確認できない | {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            detail: "サーバーエラーが発生しました".to_string(),
                        }),
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            if let Some(resp) = crate::presentation::handlers::team_archive_api::archived_conflict(&e) {
                return resp;
            }
            let msg = e.to_string();
            // バリデーションエラーの場合（repo 文言とハンドラ期待を揃える: DEMO-000169）
            if msg.contains("開始日は終了日より前") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "開始日は終了日より前でなければなりません。".to_string(),
                    }),
                )
                    .into_response()
            } else if msg.contains("teamId is required")
                || msg.contains("teamId or project is required")
                || msg.contains("project has no participating teams")
            {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "チームまたはプロジェクトを指定してください".to_string(),
                    }),
                )
                    .into_response()
            } else {
                tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response()
            }
        }
    }
}

/// PUT /api/v1/cycles/{id}/ — サイクル更新
pub async fn update(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(input): Json<CycleWriteIn>,
) -> impl IntoResponse {
    // t_cycle.team_id は NOT NULL。PUT で teamId 省略時に NULL を bind しないよう、
    // PATCH と同様に既存値で補完する（DEMO-000167）。
    let existing = match cycle_repo::find_cycle_by_id(&state.pool, id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "サイクルが見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let team_id_val = input
        .team_id
        .or_else(|| existing.team.as_ref().map(|t| t.id));
    let Some(team_id_val) = team_id_val else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "チームを指定してください".to_string(),
            }),
        )
            .into_response();
    };

    let merged = CycleWriteIn {
        team_id: Some(team_id_val),
        ..input
    };

    match cycle_repo::update_cycle(&state.pool, id, &merged).await {
        Ok(true) => {
            match cycle_repo::find_cycle_by_id(&state.pool, id).await {
                Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        detail: "サイクルが見つかりません".to_string(),
                    }),
                )
                    .into_response(),
                Err(e) => {
                    tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            detail: "サーバーエラーが発生しました".to_string(),
                        }),
                    )
                        .into_response()
                }
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            if let Some(resp) = crate::presentation::handlers::team_archive_api::archived_conflict(&e) {
                return resp;
            }
            if e.to_string().contains("開始日は終了日より前") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "開始日は終了日より前でなければなりません。".to_string(),
                    }),
                )
                    .into_response()
            } else {
                tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response()
            }
        }
    }
}

/// PATCH /api/v1/cycles/{id}/ — サイクル部分更新
///
/// DEMO-000107: フロントの useUpdateCycle は部分更新(PATCH)前提だが、本ルートは
/// 従来 PUT のみでCycleWriteIn(全フィールド必須)を要求していたため405になっていた。
/// 既存値を取得し、指定フィールドのみ上書きしてCycleWriteInを組み立てた上で、
/// ステータス遷移時のactivated_at/completed_at設定を含む既存のupdate_cycleに委譲する
/// (遷移ロジックの重複実装を避けるため)。
pub async fn patch(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(input): Json<CyclePatchIn>,
) -> impl IntoResponse {
    let existing = match cycle_repo::find_cycle_by_id(&state.pool, id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "サイクルが見つかりません".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response();
        }
    };

    let team_id_val = input.team_id
        .flatten()
        .or_else(|| existing.team.as_ref().map(|t| t.id));

    let merged = CycleWriteIn {
        project: input.project.or(existing.project),
        name: input.name.unwrap_or(existing.name),
        description: input.description.unwrap_or(existing.description),
        start_date: input.start_date.unwrap_or(existing.start_date),
        end_date: input.end_date.unwrap_or(existing.end_date),
        status: input.status.unwrap_or(existing.status),
        team_id: team_id_val,
    };

    match cycle_repo::update_cycle(&state.pool, id, &merged).await {
        Ok(true) => match cycle_repo::find_cycle_by_id(&state.pool, id).await {
            Ok(Some(cycle)) => (StatusCode::OK, Json(cycle)).into_response(),
            Ok(None) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    detail: "サイクルが見つかりません".to_string(),
                }),
            )
                .into_response(),
            Err(e) => {
                tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response()
            }
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            if let Some(resp) = crate::presentation::handlers::team_archive_api::archived_conflict(&e) {
                return resp;
            }
            if e.to_string().contains("開始日は終了日より前") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: "開始日は終了日より前でなければなりません。".to_string(),
                    }),
                )
                    .into_response()
            } else {
                tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        detail: "サーバーエラーが発生しました".to_string(),
                    }),
                )
                    .into_response()
            }
        }
    }
}

/// PATCH /api/v1/cycles/{id}/graph-position/ — 依存関係グラフ上のCycle枠の表示位置を保存
pub async fn update_graph_position(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(input): Json<CycleGraphPositionIn>,
) -> impl IntoResponse {
    match cycle_repo::update_cycle_graph_position(&state.pool, id, input.x, input.y).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("[サイクル/操作] 処理=依存関係グラフ位置更新 結果=失敗 影響=配置が保存されていない | {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// DELETE /api/v1/cycles/{id}/ — サイクル削除
pub async fn delete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cycle_repo::delete_cycle(&state.pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            if let Some(resp) = crate::presentation::handlers::team_archive_api::archived_conflict(&e) {
                return resp;
            }
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

// =============================================================================
// 高度アクション(progress / complete / velocity / burndown)
// =============================================================================

#[derive(Deserialize)]
pub struct VelocityQuery {
    pub project: Option<i32>,
}

/// GET /api/v1/cycles/{id}/progress/ — サイクル進捗
pub async fn progress(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cycle_repo::get_cycle_progress(&state.pool, id).await {
        Ok(Some(data)) => (StatusCode::OK, Json(data)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/cycles/{id}/complete/ — サイクル手動完了
pub async fn complete(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
    Json(body): Json<crate::domain::models::cycle_api::CompleteCycleIn>,
) -> impl IntoResponse {
    use cycle_repo::CompleteCycleResult;
    match cycle_repo::complete_cycle(&state.pool, id, body.carry_over_to).await {
        Ok(CompleteCycleResult::Success(data)) => (StatusCode::OK, Json(data)).into_response(),
        Ok(CompleteCycleResult::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Ok(CompleteCycleResult::AlreadyCompleted) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"completed": false, "error": "Already completed"})),
        )
            .into_response(),
        Err(e) => {
            if let Some(resp) = crate::presentation::handlers::team_archive_api::archived_conflict(&e) {
                return resp;
            }
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/cycles/velocity/?project=<id> — 直近6サイクルのベロシティデータ
pub async fn velocity(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<VelocityQuery>,
) -> impl IntoResponse {
    let project_id = match params.project {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: "project パラメータが必要です。".to_string(),
                }),
            )
                .into_response();
        }
    };

    match cycle_repo::get_velocity_data(&state.pool, project_id, 6).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/cycles/{id}/burndown/ — バーンダウンチャートデータ
pub async fn burndown(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cycle_repo::get_burndown_data(&state.pool, id).await {
        Ok(Some(data)) => (StatusCode::OK, Json(data)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                detail: "サイクルが見つかりません".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("[サイクル/操作] 処理=DB操作 結果=失敗 影響=操作が完了していない | {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: "サーバーエラーが発生しました".to_string(),
                }),
            )
                .into_response()
        }
    }
}
