//! 1 件のリソースを扱う API の認可(新しい判定と今の判定を、試運転のスイッチで切り替える)
//!
//! 詳細設計書 §9.2・§9.3。ハンドラは、本文を読む前に、ここを呼ぶ。
//! - `off`    : 今の判定だけ
//! - `shadow` : 応答は今の判定。新しい判定との違いを記録する(既定)
//! - `on`     : 新しい判定だけ。見えない物は 404、見えるが操作できない物は 403
//!
//! 今の判定が「見えない」とき、今のハンドラは 404 を返している。`shadow` / `off` でも同じ応答にする。

// 認可の失敗は、そのまま応答として返す(呼び出し側で変換しない)ため、Err に Response を持つ
#![allow(clippy::result_large_err)]

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::domain::access::{can, Action, Decision, ResourceRef, Viewer};
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Mode, Resource},
};
use crate::infrastructure::repositories::membership_repo;
use crate::presentation::state::AppState;

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"detail": "見つかりません"})),
    )
        .into_response()
}
fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"detail": "この操作の権限がありません"})),
    )
        .into_response()
}
fn server_error(e: anyhow::Error) -> Response {
    tracing::error!("[認可] 判定に失敗: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": "サーバーエラーが発生しました"})),
    )
        .into_response()
}

/// 新しい判定の結果と、今の判定の結果から、応答を決める(純粋な関数)
///
/// `current_allowed`: 今の判定で、この操作が許されるか
pub(crate) fn resolve(mode: Mode, current_allowed: bool, new: Decision) -> Result<(), Response> {
    let use_new = mode == Mode::On;
    if use_new {
        return match new {
            Decision::Allow => Ok(()),
            Decision::NotFound => Err(not_found()),
            Decision::Forbidden => Err(forbidden()),
        };
    }
    if current_allowed {
        Ok(())
    } else {
        Err(not_found())
    }
}

/// チケットの認可。見えれば判定に使った所属の情報を返す(呼び出し側で、削除の判定などに再利用できる)
///
/// 今の判定は `membership_repo::check_ticket_access`(閲覧・編集は同じ規則)。
pub async fn authorize_ticket(
    state: &AppState,
    viewer: &Viewer,
    ticket_id: i32,
    action: Action,
    route: &'static str,
) -> Result<ResourceRef, Response> {
    let facts = match facts_repo::facts_for_ticket(&state.pool, ticket_id).await {
        Ok(Some(f)) => f,
        Ok(None) => return Err(not_found()),
        Err(e) => return Err(server_error(e)),
    };
    let new = can(viewer, action, &facts);
    let mode = shadow::mode(Resource::Ticket);

    // 今の判定(off / shadow のときだけ必要)
    let current_allowed = if mode == Mode::On {
        false
    } else {
        match viewer.user_id() {
            Some(uid) => {
                match membership_repo::check_ticket_access(&state.pool, ticket_id, uid).await {
                    Ok(v) => v,
                    Err(e) => return Err(server_error(e)),
                }
            }
            None => false,
        }
    };

    if mode == Mode::Shadow && current_allowed != (new == Decision::Allow) {
        let dir = if current_allowed {
            shadow::Direction::NewlyHidden
        } else {
            shadow::Direction::NewlyVisible
        };
        shadow::record(
            &state.pool,
            Resource::Ticket,
            viewer.user_id(),
            route,
            vec![(dir, ticket_id as i64)],
        );
    }

    resolve(mode, current_allowed, new).map(|()| facts)
}

/// チケットの作成先(チーム・プロジェクト)の認可
///
/// 今の判定には、作成先の所属の確認が無い(誰でも、どのチームにも作れる)。そのため今の判定は常に「許可」とし、
/// 試運転では、新しい判定で拒否される作成を「新しく見えなくなる」として記録する(resource_id は作成先のチーム)。
/// チームが見つからない・指定が無い場合は、既存の検証(api_create)に任せる(ここでは止めない)。
pub async fn authorize_ticket_create(
    state: &AppState,
    viewer: &Viewer,
    team_id: Option<i32>,
    project_id: Option<i32>,
    route: &'static str,
) -> Result<(), Response> {
    let Some(team_id) = team_id else {
        return Ok(());
    };
    let team = match facts_repo::facts_for_team(&state.pool, team_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return Ok(()),
        Err(e) => return Err(server_error(e)),
    };
    let target = ResourceRef::Ticket {
        team: Some(team),
        project_id,
        author_id: viewer.user_id(),
        assignee_ids: vec![],
    };
    let new = can(viewer, Action::Create, &target);
    let mode = shadow::mode(Resource::Ticket);
    if mode == Mode::Shadow && new != Decision::Allow {
        shadow::record(
            &state.pool,
            Resource::Ticket,
            viewer.user_id(),
            route,
            vec![(shadow::Direction::NewlyHidden, team_id as i64)],
        );
    }
    resolve(mode, true, new)
}

/// プロジェクトの認可(今の判定に確認が無い API 用。今の判定は常に「許可」として扱う)
///
/// 試運転では、新しい判定で拒否される操作を「新しく見えなくなる」として記録する(resource_id はプロジェクト)。
/// プロジェクトが存在しない場合、`on` では 404、`off` / `shadow` では今の処理に任せる(`Ok(None)`)。
pub async fn authorize_project_open_today(
    state: &AppState,
    viewer: &Viewer,
    project_id: i32,
    action: Action,
    route: &'static str,
) -> Result<Option<ResourceRef>, Response> {
    let mode = shadow::mode(Resource::Project);
    let facts = match facts_repo::facts_for_project(&state.pool, project_id).await {
        Ok(Some(f)) => f,
        Ok(None) if mode == Mode::On => return Err(not_found()),
        Ok(None) => return Ok(None),
        Err(e) => return Err(server_error(e)),
    };
    let new = can(viewer, action, &facts);
    if mode == Mode::Shadow && new != Decision::Allow {
        shadow::record(
            &state.pool,
            Resource::Project,
            viewer.user_id(),
            route,
            vec![(shadow::Direction::NewlyHidden, project_id as i64)],
        );
    }
    resolve(mode, true, new).map(|()| Some(facts))
}

/// 一括削除の認可。対象のチケットのすべてで、新しい判定の BulkDelete が許されること
///
/// 戻り値: `on` のときは `Some(結果)`(新しい判定)。`off` / `shadow` のときは `None`(呼び出し側で、今の判定の
/// 応答をそのまま返す。今の判定は「プロジェクトのオーナーだけ」で、専用の 403 の文言を持つため)。
/// `shadow` では、今の判定と新しい判定が違えば記録する(resource_id は最初に判定が分かれたチケット)。
pub async fn authorize_bulk_delete(
    state: &AppState,
    viewer: &Viewer,
    ticket_keys: &[String],
    legacy_allowed: bool,
    route: &'static str,
) -> Option<Result<(), Response>> {
    let mode = shadow::mode(Resource::Ticket);
    if mode == Mode::Off {
        return None;
    }
    let mut new = Decision::Allow;
    let mut first_id: i64 = 0;
    for key in ticket_keys {
        let (id, facts) = match facts_repo::facts_for_ticket_key(&state.pool, key).await {
            Ok(Some(f)) => f,
            Ok(None) => continue, // 見つからない行は、既存の削除処理に任せる
            Err(e) => return Some(Err(server_error(e))),
        };
        let d = can(viewer, Action::BulkDelete, &facts);
        if d != Decision::Allow {
            new = d;
            first_id = id as i64;
            break;
        }
    }
    match mode {
        Mode::On => Some(resolve(Mode::On, legacy_allowed, new)),
        _ => {
            if legacy_allowed != (new == Decision::Allow) {
                let dir = if legacy_allowed {
                    shadow::Direction::NewlyHidden
                } else {
                    shadow::Direction::NewlyVisible
                };
                shadow::record(
                    &state.pool,
                    Resource::Ticket,
                    viewer.user_id(),
                    route,
                    vec![(dir, first_id)],
                );
            }
            None
        }
    }
}

/// 一覧の、後からの絞り込み(アクセス制御の再設計 フェーズ D)
///
/// 今の一覧は「誰でも全件」のものが多い。取得した行ごとに、新しい規則(閲覧)で見えるかを判定する。
/// - `off`    : そのまま返す
/// - `shadow` : そのまま返し、新しい規則で見えなくなる行を記録する
/// - `on`     : 見える行だけを返す
///
/// `res_of` は、行から判定用の所属(ResourceRef)を作る。閲覧の判定なので、チームの情報は
/// `viewer.team_facts_for_read(team_id)` で作ってよい(DB を読まない)。
/// 限界: 今の一覧が既に絞っている場合(所属など)、「新しく見える」行はここでは分からない。
pub fn filter_list<T>(
    pool: &sqlx::PgPool,
    viewer: &Viewer,
    resource: Resource,
    route: &'static str,
    items: Vec<T>,
    id_of: impl Fn(&T) -> i64,
    res_of: impl Fn(&T) -> ResourceRef,
) -> Vec<T> {
    let mode = shadow::mode(resource);
    if mode == Mode::Off {
        return items;
    }
    let visible = |t: &T| can(viewer, Action::Read, &res_of(t)) == Decision::Allow;
    match mode {
        Mode::On => items.into_iter().filter(&visible).collect(),
        _ => {
            let hidden: Vec<(shadow::Direction, i64)> = items
                .iter()
                .filter(|t| !visible(t))
                .map(|t| (shadow::Direction::NewlyHidden, id_of(t)))
                .collect();
            shadow::record(pool, resource, viewer.user_id(), route, hidden);
            items
        }
    }
}

/// 1 件の API の、共通の判定(アクセス制御の再設計 フェーズ D)
///
/// `legacy_allowed` は今の判定の結果(今の判定が無い API は `true`)。
/// 戻り値: `on` のときは `Some(新しい判定の結果)`。`off` / `shadow` のときは `None`
/// (呼び出し側は、今の判定の処理・応答をそのまま続ける)。`shadow` では、新旧が違えば記録する。
/// `facts` が `None`(リソースが見つからない)とき、`on` では 404、それ以外は `None`(今の処理に任せる)。
#[allow(clippy::too_many_arguments)]
pub fn enforce(
    pool: &sqlx::PgPool,
    viewer: &Viewer,
    facts: Option<&ResourceRef>,
    action: Action,
    resource: Resource,
    resource_id: i64,
    legacy_allowed: bool,
    route: &'static str,
) -> Option<Result<(), Response>> {
    let mode = shadow::mode(resource);
    if mode == Mode::Off {
        return None;
    }
    let Some(facts) = facts else {
        return (mode == Mode::On).then(|| Err(not_found()));
    };
    let new = can(viewer, action, facts);
    if mode == Mode::On {
        return Some(resolve(Mode::On, legacy_allowed, new));
    }
    if legacy_allowed != (new == Decision::Allow) {
        let dir = if legacy_allowed {
            shadow::Direction::NewlyHidden
        } else {
            shadow::Direction::NewlyVisible
        };
        shadow::record(
            pool,
            resource,
            viewer.user_id(),
            route,
            vec![(dir, resource_id)],
        );
    }
    None
}

/// 1 件の API の判定を、今の判定の応答と合わせて 1 つにする(フェーズ D の定型)
///
/// `legacy` は、今の判定の結果(今の判定の応答をそのまま持つ。確認が無い API は `Ok(())`)。
/// - `on`: 新しい判定の結果(`legacy` は使わない)
/// - `off` / `shadow`: `legacy`(`shadow` では、新旧が違えば記録する)
#[allow(clippy::too_many_arguments)]
pub fn gate(
    pool: &sqlx::PgPool,
    viewer: &Viewer,
    facts: Option<&ResourceRef>,
    action: Action,
    resource: Resource,
    resource_id: i64,
    legacy: Result<(), Response>,
    route: &'static str,
) -> Result<(), Response> {
    match enforce(
        pool,
        viewer,
        facts,
        action,
        resource,
        resource_id,
        legacy.is_ok(),
        route,
    ) {
        Some(new) => new,
        None => legacy,
    }
}

/// プロジェクト 1 件の判定(`gate` の、プロジェクトの所属を読む版)
pub async fn gate_project(
    pool: &sqlx::PgPool,
    viewer: &Viewer,
    project_id: i32,
    action: Action,
    legacy: Result<(), Response>,
    route: &'static str,
) -> Result<(), Response> {
    let facts = facts_repo::facts_for_project(pool, project_id)
        .await
        .map_err(server_error)?;
    gate(
        pool,
        viewer,
        facts.as_ref(),
        action,
        Resource::Project,
        project_id as i64,
        legacy,
        route,
    )
}

/// チーム 1 件の判定(`gate` の、チームの情報を読む版)
pub async fn gate_team(
    pool: &sqlx::PgPool,
    viewer: &Viewer,
    team_id: i32,
    action: Action,
    legacy: Result<(), Response>,
    route: &'static str,
) -> Result<(), Response> {
    let facts = facts_repo::facts_for_team(pool, team_id)
        .await
        .map_err(server_error)?
        .map(ResourceRef::Team);
    gate(
        pool,
        viewer,
        facts.as_ref(),
        action,
        Resource::Team,
        team_id as i64,
        legacy,
        route,
    )
}

/// チーム・プロジェクト・全体のどれかに属する物(ラベル・マイルストーン・ワークフローの状態など)の判定(D-4)
///
/// `on_scoped`: チーム・プロジェクトに属する物のときの操作(設定なら ManageSettings、マイルストーンなら Write)。
/// 全体の物(GlobalMaster)は `action` をそのまま使う(変更はシステム管理者だけ)。閲覧(Read)は常に Read。
/// 今の判定には確認が無い前提(`legacy` は `Ok(())`)。
#[allow(clippy::too_many_arguments)]
pub async fn gate_scoped(
    pool: &sqlx::PgPool,
    viewer: &Viewer,
    team_id: Option<i32>,
    project_id: Option<i32>,
    action: Action,
    on_scoped: Action,
    resource: Resource,
    id: i32,
    route: &'static str,
) -> Result<(), Response> {
    let facts = facts_repo::facts_for_scoped(pool, team_id, project_id)
        .await
        .map_err(server_error)?;
    let action = match (&facts, action) {
        (_, Action::Read) => Action::Read,
        (Some(ResourceRef::GlobalMaster), a) => a,
        _ => on_scoped,
    };
    gate(
        pool,
        viewer,
        facts.as_ref(),
        action,
        resource,
        id as i64,
        Ok(()),
        route,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_follows_the_switch() {
        // 試運転・off: 今の判定の答え(見えない = 404)
        for mode in [Mode::Shadow, Mode::Off] {
            assert!(resolve(mode, true, Decision::NotFound).is_ok());
            assert_eq!(
                resolve(mode, false, Decision::Allow).unwrap_err().status(),
                StatusCode::NOT_FOUND
            );
        }
        // on: 新しい判定の答え
        assert!(resolve(Mode::On, false, Decision::Allow).is_ok());
        assert_eq!(
            resolve(Mode::On, true, Decision::NotFound)
                .unwrap_err()
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            resolve(Mode::On, true, Decision::Forbidden)
                .unwrap_err()
                .status(),
            StatusCode::FORBIDDEN
        );
    }

    fn fm_viewer(public: Vec<i32>, member_of: Vec<i32>) -> Viewer {
        use crate::domain::access::{Membership, MembershipKind, Principal, Role};
        Viewer {
            principal: Principal::Human { user_id: 1 },
            role: Role::FullMember,
            memberships: member_of
                .into_iter()
                .map(|team_id| Membership {
                    team_id,
                    kind: MembershipKind::Team,
                    is_owner: false,
                    expires_on: None,
                })
                .collect(),
            public_team_ids: public,
        }
    }

    #[tokio::test]
    async fn filter_list_keeps_everything_in_shadow_and_records_hidden() {
        let Some(pool) = crate::test_support::test_pool().await else {
            return;
        };
        // チーム 1 は Public、2 は Private(未所属)、3 は Private(所属)
        let v = fm_viewer(vec![1], vec![3]);
        let items = vec![(101_i64, 1), (102, 2), (103, 3)];
        let got = filter_list(
            &pool,
            &v,
            Resource::Cycle,
            "test-filter",
            items.clone(),
            |t| t.0,
            |t| ResourceRef::Cycle {
                team: v.team_facts_for_read(t.1),
            },
        );
        assert_eq!(got, items, "試運転では、今と同じ一覧を返す");
        let mut n = 0i64;
        for _ in 0..50 {
            n = sqlx::query_scalar("SELECT count(*) FROM access_shadow_diff WHERE route = 'test-filter' AND resource_id = 102")
                .fetch_one(&pool)
                .await
                .unwrap();
            if n > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(n, 1, "見えなくなる行(Private・未所属)だけが記録される");
    }

    #[test]
    fn visible_rows_follow_the_policy() {
        // on のときに残る行と同じ判定(filter_list の中の判定)を、直接確かめる
        let v = fm_viewer(vec![1], vec![3]);
        let visible: Vec<i32> = [1, 2, 3]
            .into_iter()
            .filter(|&t| {
                can(
                    &v,
                    Action::Read,
                    &ResourceRef::Cycle {
                        team: v.team_facts_for_read(t),
                    },
                ) == Decision::Allow
            })
            .collect();
        assert_eq!(visible, vec![1, 3]);
    }
}
