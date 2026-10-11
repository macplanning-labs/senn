//! AI 用 API の追加分(DEMO-000217〜000222)の、ルーターを通した HTTP テスト。
//! 作成時の項目・ラベルの名前での更新・一覧の絞り込み・一括登録・チームの Prefix。
use crate::infrastructure::repositories::ai_agent_key_repo;
use crate::test_support::{
    add_test_team_member, create_test_project, create_test_ticket, create_test_user, test_config,
    test_pool,
};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

const KEY: &str = "test-ai-mcp-ext-key-0123456789";

async fn app(pool: &PgPool) -> axum::Router {
    let mut cfg = test_config();
    cfg.wip_ai_api_key = Some(KEY.to_string());
    let state = crate::presentation::state::AppState::new(pool.clone(), cfg, None)
        .await
        .expect("AppState");
    crate::routes::create_router(state)
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    key: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("X-AI-Api-Key", key)
        .body(match body {
            Some(b) => Body::from(b.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:40000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// プロジェクト(状態 4 つ)と、その参加チーム・既存のチケット 1 件
struct Fixture {
    prefix: String,
    project_id: i32,
    team_id: i32,
    existing_key: String,
    user: i32,
}

async fn fixture(pool: &PgPool, base: &str) -> Fixture {
    let user = create_test_user(pool, "ai-mcp").await;
    let project_id = create_test_project(pool, base, user).await;
    for (slug, cat, pos) in [
        ("backlog", "backlog", 0),
        ("open", "unstarted", 1),
        ("in_progress", "started", 2),
        ("resolved", "completed", 3),
    ] {
        sqlx::query(
            "INSERT INTO t_workflow_status (slug, name, category, color, position, is_default, project_id)
             VALUES ($1, $1, $2, '#000000', $3, false, $4)
             ON CONFLICT (project_id, slug) WHERE project_id IS NOT NULL DO NOTHING",
        )
        .bind(slug)
        .bind(cat)
        .bind(pos)
        .bind(i64::from(project_id))
        .execute(pool)
        .await
        .unwrap();
    }
    let prefix: String = sqlx::query_scalar("SELECT prefix FROM tickets_project WHERE id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap();
    let team_id: i32 = sqlx::query_scalar(
        "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let tid = create_test_ticket(pool, project_id, &prefix, user).await;
    let existing_key: String =
        sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(tid)
            .fetch_one(pool)
            .await
            .unwrap();
    Fixture {
        prefix,
        project_id,
        team_id,
        existing_key,
        user,
    }
}

async fn create_cycle(pool: &PgPool, project_id: i32, team_id: i32) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO t_cycle (project_id, team_id, name, number, description, start_date, end_date, status, created_at)
         VALUES ($1, $2, 'c', 1, '', CURRENT_DATE, CURRENT_DATE + 7, 'planned', NOW()) RETURNING id::int4",
    )
    .bind(i64::from(project_id))
    .bind(i64::from(team_id))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn count_tickets(pool: &PgPool, project_id: i32) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM tickets_ticket WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn labels_of(pool: &PgPool, key: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT l.name FROM tickets_ticket_labels tl JOIN m_label l ON l.id = tl.labelmodel_id
         JOIN tickets_ticket t ON t.id = tl.ticketmodel_id WHERE t.ticket_key = $1 ORDER BY l.name",
    )
    .bind(key)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// 000218: 1 回の作成で、状態・ストーリーポイント・Cycle・親・開始日まで入る。着手の状態なら開始日が入る
#[tokio::test]
async fn create_ticket_accepts_status_points_cycle_and_parent() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let f = fixture(&pool, "AMC").await;
    let cycle = create_cycle(&pool, f.project_id, f.team_id).await;
    let app = app(&pool).await;

    let (st, body) = call(
        &app,
        "POST",
        "/api/v1/ai-agent/tickets/",
        KEY,
        Some(json!({
            "project_prefix": f.prefix, "title": "作成時に全部入る",
            "status": "in_progress", "story_points": 3, "cycle": cycle,
            "parent_key": f.existing_key,
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
    let key = body["ticket_key"].as_str().unwrap().to_string();
    let row: (
        String,
        Option<i16>,
        Option<i32>,
        Option<i32>,
        Option<chrono::NaiveDate>,
    ) = sqlx::query_as(
        "SELECT t.status, t.story_points, t.cycle_id::int4, t.parent_id::int4, t.start_date
             FROM tickets_ticket t WHERE t.ticket_key = $1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .unwrap();
    let parent_id: i32 =
        sqlx::query_scalar("SELECT id::int4 FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&f.existing_key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "in_progress");
    assert_eq!(row.1, Some(3));
    assert_eq!(row.2, Some(cycle));
    assert_eq!(row.3, Some(parent_id));
    assert!(row.4.is_some(), "着手の状態で作ったので、開始日が入る");
}

/// 000218: 登録の無い状態(done など)・フィボナッチでないポイント・よそのチームの Cycle は 400 で、何も作らない
#[tokio::test]
async fn create_ticket_rejects_unknown_status_points_and_foreign_cycle() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let f = fixture(&pool, "AMR").await;
    let other = fixture(&pool, "AMO").await;
    let foreign_cycle = create_cycle(&pool, other.project_id, other.team_id).await;
    let app = app(&pool).await;
    let before = count_tickets(&pool, f.project_id).await;

    for (extra, needle) in [
        (json!({"status": "done"}), "使える状態"),
        (json!({"story_points": 4}), "story_points"),
        (json!({"cycle": foreign_cycle}), "Cycle"),
        (json!({"parent": 1, "parent_key": f.existing_key}), "parent"),
    ] {
        let mut body = json!({"project_prefix": f.prefix, "title": "x"});
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (st, resp) = call(&app, "POST", "/api/v1/ai-agent/tickets/", KEY, Some(body)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{resp}");
        assert!(
            resp["error"].as_str().unwrap_or("").contains(needle),
            "{needle}: {resp}"
        );
    }
    assert_eq!(count_tickets(&pool, f.project_id).await, before);
}

/// 000219: 更新で、ラベルを名前で足す・外す・置き換える。番号と名前の同時指定は 400。登録の無い状態も 400
#[tokio::test]
async fn patch_ticket_labels_by_name() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let f = fixture(&pool, "AML").await;
    let app = app(&pool).await;
    let uri = format!("/api/v1/ai-agent/tickets/{}/", f.existing_key);

    let (st, b) = call(
        &app,
        "PATCH",
        &uri,
        KEY,
        Some(json!({"add_labels": ["a", "b"]})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(labels_of(&pool, &f.existing_key).await, vec!["a", "b"]);

    let (st, b) = call(
        &app,
        "PATCH",
        &uri,
        KEY,
        Some(json!({"add_labels": ["c"], "remove_labels": ["a", "無い"]})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(labels_of(&pool, &f.existing_key).await, vec!["b", "c"]);

    let (st, b) = call(
        &app,
        "PATCH",
        &uri,
        KEY,
        Some(json!({"label_names": ["z"]})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(labels_of(&pool, &f.existing_key).await, vec!["z"]);

    let (st, _) = call(
        &app,
        "PATCH",
        &uri,
        KEY,
        Some(json!({"labels": [], "add_labels": ["q"]})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(&app, "PATCH", &uri, KEY, Some(json!({"status": "done"}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // 名前の指定以外の更新は、今までどおり
    let (st, b) = call(&app, "PATCH", &uri, KEY, Some(json!({"title": "新しい題"}))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(
        labels_of(&pool, &f.existing_key).await,
        vec!["z"],
        "ラベルは触らない"
    );
}

/// 000222: 一覧を、状態・除く状態・ラベル・Cycle・担当者・更新日時で絞る
#[tokio::test]
async fn list_tickets_filters() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let f = fixture(&pool, "AMF").await;
    let cycle = create_cycle(&pool, f.project_id, f.team_id).await;
    let app = app(&pool).await;
    let username: String = sqlx::query_scalar("SELECT username FROM accounts_user WHERE id = $1")
        .bind(f.user)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut keys = Vec::new();
    for (status, label, in_cycle, assignee) in [
        ("open", "x", true, true),
        ("resolved", "x", true, false),
        ("in_progress", "y", false, false),
    ] {
        let mut body = json!({"project_prefix": f.prefix, "title": status, "status": status, "labels": [label]});
        if in_cycle {
            body["cycle"] = json!(cycle);
        }
        if assignee {
            body["assignee_usernames"] = json!([username]);
        }
        let (st, b) = call(&app, "POST", "/api/v1/ai-agent/tickets/", KEY, Some(body)).await;
        assert_eq!(st, StatusCode::CREATED, "{b}");
        keys.push(b["ticket_key"].as_str().unwrap().to_string());
    }
    let list = |q: String| {
        let app = app.clone();
        let prefix = f.prefix.clone();
        async move {
            let (st, b) = call(
                &app,
                "GET",
                &format!("/api/v1/ai-agent/tickets/?project_prefix={prefix}&{q}"),
                KEY,
                None,
            )
            .await;
            assert_eq!(st, StatusCode::OK, "{b}");
            let mut v: Vec<String> = b
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["ticketKey"].as_str().unwrap().to_string())
                .collect();
            v.sort();
            v
        }
    };
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    // この Cycle の未完了
    assert_eq!(
        list(format!("cycle={cycle}&status_not=resolved,closed,canceled")).await,
        vec![keys[0].clone()]
    );
    assert_eq!(
        list("status=open,in_progress&label=x".into()).await,
        vec![keys[0].clone()]
    );
    assert_eq!(list("label=y".into()).await, vec![keys[2].clone()]);
    assert_eq!(
        list(format!("assignee={username}")).await,
        vec![keys[0].clone()]
    );
    assert!(list("updated_after=2999-01-01".into()).await.is_empty());
    let all = list("updated_after=2000-01-01T00:00:00%2B09:00".into()).await;
    assert_eq!(all.len(), 4, "既存の 1 件 + 作った 3 件: {all:?}");
    assert!(sorted(keys.clone()).iter().all(|k| all.contains(k)));

    let (st, _) = call(
        &app,
        "GET",
        &format!(
            "/api/v1/ai-agent/tickets/?project_prefix={}&updated_after=昨日",
            f.prefix
        ),
        KEY,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// 000221: 依存関係ごとまとめて登録する。試しに実行・循環・不正な状態では、何も登録しない
#[tokio::test]
async fn bulk_create_is_all_or_nothing() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let f = fixture(&pool, "AMB").await;
    let app = app(&pool).await;
    let uri = "/api/v1/ai-agent/tickets/bulk/";
    let plan = |dry_run: bool| {
        json!({
            "project_prefix": f.prefix,
            "dry_run": dry_run,
            "tickets": [
                {"ref": "T1", "title": "設計", "labels": ["一括"]},
                {"ref": "T2", "title": "実装", "depends_on": ["T1"], "parent": "T1"},
                {"ref": "T3", "title": "確認", "depends_on": ["T2", f.existing_key], "status": "in_progress"},
            ]
        })
    };
    let before = count_tickets(&pool, f.project_id).await;
    let labels_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM m_label WHERE project_id = $1")
            .bind(f.project_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // 試しに実行: 結果は返るが、何も残らない(ラベルも作らない)
    let (st, b) = call(&app, "POST", uri, KEY, Some(plan(true))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["dry_run"], json!(true));
    assert_eq!(b["tickets"].as_array().unwrap().len(), 3);
    assert_eq!(b["dependencies"].as_array().unwrap().len(), 3);
    assert_eq!(count_tickets(&pool, f.project_id).await, before);
    let labels_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM m_label WHERE project_id = $1")
            .bind(f.project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        labels_after, labels_before,
        "試しに実行ではラベルも作らない"
    );

    // 循環: T1 → T2 → T1
    let (st, b) = call(
        &app,
        "POST",
        uri,
        KEY,
        Some(json!({"project_prefix": f.prefix, "tickets": [
            {"ref": "A", "title": "a", "depends_on": ["B"]},
            {"ref": "B", "title": "b", "depends_on": ["A"]},
        ]})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert!(b["error"].as_str().unwrap().contains("循環"), "{b}");
    // 後ろの 1 件だけ状態が不正
    let mut bad = plan(false);
    bad["tickets"][2]["status"] = json!("done");
    let (st, b) = call(&app, "POST", uri, KEY, Some(bad)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert!(b["error"].as_str().unwrap().contains("ref: T3"), "{b}");
    // 存在しない参照
    let (st, _) = call(
        &app,
        "POST",
        uri,
        KEY,
        Some(json!({"project_prefix": f.prefix, "tickets": [
            {"ref": "A", "title": "a", "depends_on": ["NOPE-999999"]},
        ]})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(count_tickets(&pool, f.project_id).await, before);

    // 本番: 3 件と依存 3 本、親子 1 つ
    let (st, b) = call(&app, "POST", uri, KEY, Some(plan(false))).await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
    assert_eq!(count_tickets(&pool, f.project_id).await, before + 3);
    let key_of = |r: &str| {
        b["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["ref"] == r)
            .unwrap()["ticket_key"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (t1, t2, t3) = (key_of("T1"), key_of("T2"), key_of("T3"));
    let edges: Vec<(String, String)> = sqlx::query_as(
        "SELECT ft.ticket_key, tt.ticket_key FROM t_task_dependency d
         JOIN tickets_ticket ft ON ft.id = d.from_task_id JOIN tickets_ticket tt ON tt.id = d.to_task_id
         WHERE tt.ticket_key = ANY($1) AND d.dependency_type = 'blocks' ORDER BY 1, 2",
    )
    .bind(vec![t1.clone(), t2.clone(), t3.clone()])
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut expected = vec![
        (t1.clone(), t2.clone()),
        (t2.clone(), t3.clone()),
        (f.existing_key.clone(), t3.clone()),
    ];
    expected.sort();
    assert_eq!(edges, expected, "先に終わらせるもの → 後のもの");
    let parent_of_t2: Option<String> = sqlx::query_scalar(
        "SELECT p.ticket_key FROM tickets_ticket c JOIN tickets_ticket p ON p.id = c.parent_id WHERE c.ticket_key = $1",
    )
    .bind(&t2)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(parent_of_t2, Some(t1.clone()));
    assert_eq!(labels_of(&pool, &t1).await, vec!["一括"]);
    let started: Option<chrono::NaiveDate> =
        sqlx::query_scalar("SELECT start_date FROM tickets_ticket WHERE ticket_key = $1")
            .bind(&t3)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(started.is_some(), "着手で作ったものは開始日が入る");
}

/// 000220: 依存の種類は blocks / relates_to だけ
#[tokio::test]
async fn add_dependency_rejects_unknown_type() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let f = fixture(&pool, "AMD").await;
    let other = create_test_ticket(&pool, f.project_id, &f.prefix, f.user).await;
    let other_key: String =
        sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(other)
            .fetch_one(&pool)
            .await
            .unwrap();
    let app = app(&pool).await;
    let uri = format!("/api/v1/ai-agent/tickets/{}/dependencies/", f.existing_key);
    let (st, b) = call(
        &app,
        "POST",
        &uri,
        KEY,
        Some(json!({"to_task_key": other_key, "dependency_type": "blocked_by"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    let (st, b) = call(
        &app,
        "POST",
        &uri,
        KEY,
        Some(json!({"to_task_key": other_key, "dependency_type": "relates_to"})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
}

/// 000217: Prefix を指定してチームを作れる。重複は、どのチームと重なったかが分かる 409。
/// 変更はチームの管理者の個人キーだけ(共有キー・管理者でない人は 403)
#[tokio::test]
async fn team_prefix_on_create_and_patch() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let app = app(&pool).await;
    let suffix = crate::test_support::unique_suffix();
    let prefix = format!("P{}", &suffix[suffix.len().saturating_sub(8)..]).to_uppercase();
    let (st, team) = call(
        &app,
        "POST",
        "/api/v1/ai-agent/teams/",
        KEY,
        Some(json!({"name": "Prefixテスト", "slug": format!("pfx-{suffix}"), "prefix": prefix.to_lowercase()})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{team}");
    assert_eq!(
        team["prefix"].as_str(),
        Some(prefix.as_str()),
        "大文字にそろえる"
    );
    let team_id = team["id"].as_i64().unwrap() as i32;

    let (st, b) = call(
        &app,
        "POST",
        "/api/v1/ai-agent/teams/",
        KEY,
        Some(json!({"name": "重複", "slug": format!("pfx2-{suffix}"), "prefix": prefix})),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{b}");
    assert!(b["error"].as_str().unwrap().contains("Prefixテスト"), "{b}");
    let (st, _) = call(
        &app,
        "POST",
        "/api/v1/ai-agent/teams/",
        KEY,
        Some(json!({"name": "形", "prefix": "AB-C"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // 変更: 管理者の個人キー
    let admin = create_test_user(&pool, "pfx-admin").await;
    let outsider = create_test_user(&pool, "pfx-out").await;
    add_test_team_member(&pool, team_id, admin, "admin").await;
    let admin_key = format!("senn_ai_admin_{suffix}");
    let out_key = format!("senn_ai_out_{suffix}");
    ai_agent_key_repo::create(
        &pool,
        admin,
        &ai_agent_key_repo::hash_key(&admin_key),
        "senn_ai_a",
        None,
        admin,
    )
    .await
    .unwrap();
    ai_agent_key_repo::create(
        &pool,
        outsider,
        &ai_agent_key_repo::hash_key(&out_key),
        "senn_ai_o",
        None,
        outsider,
    )
    .await
    .unwrap();
    let new_prefix = format!("Q{}", &prefix[1..]);
    let uri = format!("/api/v1/ai-agent/teams/{team_id}/");
    let patch = json!({"prefix": new_prefix});
    let (st, _) = call(&app, "PATCH", &uri, KEY, Some(patch.clone())).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "共有キーは不可");
    let (st, _) = call(&app, "PATCH", &uri, &out_key, Some(patch.clone())).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "管理者でない人は不可");
    let slug_uri = format!("/api/v1/ai-agent/teams/pfx-{suffix}/");
    let (st, b) = call(&app, "PATCH", &slug_uri, &admin_key, Some(patch)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["prefix"].as_str(), Some(new_prefix.as_str()));
    assert_eq!(
        b["name"].as_str(),
        Some("Prefixテスト"),
        "指定の無い項目は変えない"
    );
}
