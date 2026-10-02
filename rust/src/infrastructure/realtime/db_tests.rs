//! 実際の DB に対するテスト（マイグレーションのトリガー・通知・配信役の通し）。
//! DB が無い環境では test_pool() が None を返して何も検証せずに終わる。REQUIRE_TEST_DB=1 で必須にできる。
use std::time::Duration;

use sqlx::postgres::PgListener;
use sqlx::PgPool;
use tokio::sync::mpsc;

use super::dispatcher::{dispatch, Notice};
use super::hub::{Frame, Hub};
use crate::domain::models::realtime::RoomId;
use crate::test_support::{
    create_test_project, create_test_team, create_test_ticket, create_test_user, test_pool,
};

async fn ticket_version(pool: &PgPool, id: i32) -> i64 {
    sqlx::query_scalar("SELECT sync_version FROM tickets_ticket WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn setup(pool: &PgPool) -> (i32, i32, i32) {
    let user = create_test_user(pool, "rt-user").await;
    let project = create_test_project(pool, "RT", user).await;
    let ticket = create_test_ticket(pool, project, "RT", user).await;
    (user, project, ticket)
}

async fn team_of(pool: &PgPool, ticket: i32) -> i32 {
    sqlx::query_scalar("SELECT team_id::int4 FROM tickets_ticket WHERE id = $1")
        .bind(ticket)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn drain(rx: &mut mpsc::Receiver<Frame>) -> Vec<serde_json::Value> {
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        out.push(serde_json::from_str(&f).unwrap());
    }
    out
}

#[tokio::test]
async fn version_increments_on_every_update_and_tombstone_records_next_version() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (_, _, ticket) = setup(&pool).await;
    assert_eq!(ticket_version(&pool, ticket).await, 1, "INSERT 直後は 1");

    sqlx::query("UPDATE tickets_ticket SET title = 'a' WHERE id = $1")
        .bind(ticket)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(ticket_version(&pool, ticket).await, 2);
    // 内容が変わらない更新（表示名の連鎖更新と同じ sync_changed_at だけの更新）でも版は進む
    sqlx::query("UPDATE tickets_ticket SET sync_changed_at = clock_timestamp() WHERE id = $1")
        .bind(ticket)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(ticket_version(&pool, ticket).await, 3);

    sqlx::query("DELETE FROM tickets_ticket WHERE id = $1")
        .bind(ticket)
        .execute(&pool)
        .await
        .unwrap();
    let v: i64 = sqlx::query_scalar(
        "SELECT sync_version FROM sync_tombstones WHERE entity = 'ticket' AND entity_id = $1",
    )
    .bind(ticket as i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        v, 4,
        "削除記録の版 = 削除直前の版 + 1（どの upsert より新しい）"
    );
}

#[tokio::test]
async fn notify_is_sent_on_commit_only_and_carries_place_and_version() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (user, project, _) = setup(&pool).await;
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("senn_sync").await.unwrap();

    // ロールバックした変更は届かない
    let mut tx = pool.begin().await.unwrap();
    let rolled_back = create_test_ticket_in_tx(&mut tx, project, user).await;
    tx.rollback().await.unwrap();
    // コミットした変更は届く
    let committed = create_test_ticket(&pool, project, "RTC", user).await;

    let mut got = Vec::new();
    while let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(500), listener.recv()).await {
        got.push(serde_json::from_str::<serde_json::Value>(n.payload()).unwrap());
    }
    assert!(
        got.iter()
            .filter(|p| p["e"] == "ticket")
            .all(|p| p["id"].as_i64() != Some(rolled_back as i64)),
        "ロールバックした行の通知が届いた: {got:?}"
    );
    let mine = got
        .iter()
        .find(|p| p["e"] == "ticket" && p["id"].as_i64() == Some(committed as i64))
        .expect("コミットした行の通知");
    assert_eq!(mine["e"], "ticket");
    assert_eq!(mine["a"], "insert");
    assert_eq!(mine["v"], 1);
    assert_eq!(mine["p"], project);
    assert_eq!(mine["t"], team_of(&pool, committed).await);
}

async fn create_test_ticket_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project: i32,
    user: i32,
) -> i32 {
    let team: i32 = sqlx::query_scalar(
        "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
    )
    .bind(project)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO tickets_ticket (ticket_key, title, description, status, priority, ticket_type, project_id, author_id, team_id, created_at, updated_at, gantt_order)
         VALUES ('RTRB-' || substr(md5(random()::text), 1, 8), 'x', '', 'open', 'medium', 'task', $1, $2, $3, NOW(), NOW(), 0) RETURNING id::int4",
    )
    .bind(project)
    .bind(user)
    .bind(team)
    .fetch_one(&mut **tx)
    .await
    .unwrap()
}

#[tokio::test]
async fn dispatch_pushes_full_row_to_team_room_only() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (_, _, ticket) = setup(&pool).await;
    let team = team_of(&pool, ticket).await;
    let other_team = create_test_team(&pool, "rt-other").await;

    let hub = Hub::new();
    let (tx_member, mut rx_member) = mpsc::channel(16);
    let (tx_other, mut rx_other) = mpsc::channel(16);
    let (tx_staff, mut rx_staff) = mpsc::channel(16);
    hub.join(1, tx_member, vec![RoomId::team(team)]).await;
    hub.join(2, tx_other, vec![RoomId::team(other_team)]).await;
    hub.join(3, tx_staff, vec![RoomId::all()]).await;

    sqlx::query("UPDATE tickets_ticket SET title = '変更後のタイトル' WHERE id = $1")
        .bind(ticket)
        .execute(&pool)
        .await
        .unwrap();
    let v = ticket_version(&pool, ticket).await;
    let notice = Notice {
        e: "ticket".into(),
        a: "update".into(),
        id: ticket as i64,
        v,
        t: Some(team),
        p: None,
        ot: None,
        opj: None,
        u: None,
        teams: vec![],
    };
    dispatch(&pool, &hub, vec![notice], "ai_agent")
        .await
        .unwrap();

    let member = drain(&mut rx_member).await;
    assert_eq!(member.len(), 1, "{member:?}");
    assert_eq!(member[0]["room"], format!("t:{team}"));
    assert_eq!(member[0]["seq"], 1);
    let change = &member[0]["changes"][0];
    assert_eq!(change["op"], "upsert");
    assert_eq!(change["v"], v);
    assert_eq!(change["data"]["title"], "変更後のタイトル");
    assert_eq!(change["data"]["v"], v, "差分同期と同じ DTO（v を含む）");
    assert!(
        change["data"]["description"].is_string(),
        "sync 専用項目を含む"
    );

    assert!(
        drain(&mut rx_other).await.is_empty(),
        "別チームには届かない"
    );
    let staff = drain(&mut rx_staff).await;
    assert_eq!(staff[0]["room"], "all");
}

#[tokio::test]
async fn moving_a_ticket_evicts_it_from_the_old_team_room() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (_, project, ticket) = setup(&pool).await;
    let old_team = team_of(&pool, ticket).await;
    let new_team = create_test_team(&pool, "rt-new").await;
    // 移動先チームはプロジェクトの参加チームでなければならない（DB のルール）
    sqlx::query(
        "INSERT INTO tickets_project_teams (project_id, team_id, joined_at) VALUES ($1, $2, NOW())",
    )
    .bind(project)
    .bind(new_team)
    .execute(&pool)
    .await
    .unwrap();

    let hub = Hub::new();
    let (tx_old, mut rx_old) = mpsc::channel(16);
    let (tx_new, mut rx_new) = mpsc::channel(16);
    hub.join(1, tx_old, vec![RoomId::team(old_team)]).await;
    hub.join(2, tx_new, vec![RoomId::team(new_team)]).await;

    sqlx::query("UPDATE tickets_ticket SET team_id = $2 WHERE id = $1")
        .bind(ticket)
        .bind(new_team)
        .execute(&pool)
        .await
        .unwrap();
    let v = ticket_version(&pool, ticket).await;
    let notice = Notice {
        e: "ticket".into(),
        a: "update".into(),
        id: ticket as i64,
        v,
        t: Some(new_team),
        p: None,
        ot: Some(old_team),
        opj: None,
        u: None,
        teams: vec![],
    };
    dispatch(&pool, &hub, vec![notice], "ai_agent")
        .await
        .unwrap();

    let old = drain(&mut rx_old).await;
    assert_eq!(old[0]["changes"][0]["op"], "evict", "{old:?}");
    assert_eq!(old[0]["changes"][0]["id"], ticket);
    let new = drain(&mut rx_new).await;
    assert_eq!(new[0]["changes"][0]["op"], "upsert");
}

#[tokio::test]
async fn deleting_a_ticket_pushes_delete_without_refetching() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (_, _, ticket) = setup(&pool).await;
    let team = team_of(&pool, ticket).await;
    let hub = Hub::new();
    let (tx, mut rx) = mpsc::channel(16);
    hub.join(1, tx, vec![RoomId::team(team)]).await;

    let v = ticket_version(&pool, ticket).await;
    sqlx::query("DELETE FROM tickets_ticket WHERE id = $1")
        .bind(ticket)
        .execute(&pool)
        .await
        .unwrap();
    let notice = Notice {
        e: "ticket".into(),
        a: "delete".into(),
        id: ticket as i64,
        v: v + 1,
        t: Some(team),
        p: None,
        ot: None,
        opj: None,
        u: None,
        teams: vec![],
    };
    dispatch(&pool, &hub, vec![notice], "ai_agent")
        .await
        .unwrap();

    let got = drain(&mut rx).await;
    assert_eq!(got[0]["changes"][0]["op"], "delete");
    assert_eq!(got[0]["changes"][0]["v"], v + 1);
}

#[tokio::test]
async fn project_change_is_a_signal_to_everyone() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (_, project, _) = setup(&pool).await;
    let hub = Hub::new();
    let (tx, mut rx) = mpsc::channel(16);
    hub.join(1, tx, vec![super::rooms::global_room()]).await;
    let notice = Notice {
        e: "project".into(),
        a: "update".into(),
        id: project as i64,
        v: 2,
        t: None,
        p: None,
        ot: None,
        opj: None,
        u: None,
        teams: vec![],
    };
    dispatch(&pool, &hub, vec![notice], "ai_agent")
        .await
        .unwrap();
    let got = drain(&mut rx).await;
    assert_eq!(got[0]["room"], "g");
    assert_eq!(got[0]["changes"][0]["op"], "stale");
    assert_eq!(got[0]["changes"][0]["entity"], "project");
}

#[tokio::test]
async fn connect_token_can_be_consumed_exactly_once_and_not_after_expiry() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let user = create_test_user(&pool, "rt-token").await;
    let hash = uuid::Uuid::new_v4().as_bytes().to_vec();
    let expired = uuid::Uuid::new_v4().as_bytes().to_vec();
    sqlx::query("INSERT INTO realtime_connect_tokens (token_hash, user_id, expires_at) VALUES ($1, $2, NOW() + interval '30 seconds')")
        .bind(&hash).bind(user).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO realtime_connect_tokens (token_hash, user_id, expires_at) VALUES ($1, $2, NOW() - interval '1 second')")
        .bind(&expired).bind(user).execute(&pool).await.unwrap();

    let sql = "DELETE FROM realtime_connect_tokens WHERE token_hash = $1 AND expires_at > NOW() RETURNING user_id";
    // 同時に2つ来ても、取れるのは1つだけ
    let (a, b) = tokio::join!(
        sqlx::query_scalar::<_, i32>(sql)
            .bind(&hash)
            .fetch_optional(&pool),
        sqlx::query_scalar::<_, i32>(sql)
            .bind(&hash)
            .fetch_optional(&pool),
    );
    let taken = [a.unwrap(), b.unwrap()].into_iter().flatten().count();
    assert_eq!(taken, 1, "同じトークンは1回しか使えない");
    assert!(
        sqlx::query_scalar::<_, i32>(sql)
            .bind(&expired)
            .fetch_optional(&pool)
            .await
            .unwrap()
            .is_none(),
        "期限切れは使えない"
    );
}

// ── コメント（Phase 2） ──

async fn add_comment(pool: &PgPool, ticket: i32, author: i32, body: &str) -> i32 {
    sqlx::query_scalar("INSERT INTO tickets_comment (body, author_id, ticket_id, created_at) VALUES ($1, $2, $3, NOW()) RETURNING id::int4")
        .bind(body)
        .bind(author)
        .bind(ticket)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn comment_version(pool: &PgPool, id: i32) -> i64 {
    sqlx::query_scalar("SELECT sync_version FROM tickets_comment WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn make_member(pool: &PgPool, team: i32) -> i32 {
    let user = create_test_user(pool, "rt-member").await;
    sqlx::query("INSERT INTO t_team_membership (role, joined_at, user_id, team_id) VALUES ('member', NOW(), $1, $2)")
        .bind(user)
        .bind(team)
        .execute(pool)
        .await
        .unwrap();
    user
}

#[tokio::test]
async fn comment_versions_and_ticket_is_bumped_on_insert_and_delete() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (user, _, ticket) = setup(&pool).await;
    let v0 = ticket_version(&pool, ticket).await;

    let c = add_comment(&pool, ticket, user, "はじめ").await;
    assert_eq!(comment_version(&pool, c).await, 1);
    assert_eq!(
        ticket_version(&pool, ticket).await,
        v0 + 1,
        "コメント追加でチケット行の版も進む（一覧のコメント数が変わる）"
    );

    sqlx::query("UPDATE tickets_comment SET body = '編集', updated_at = NOW() WHERE id = $1")
        .bind(c)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(comment_version(&pool, c).await, 2);
    sqlx::query("UPDATE tickets_comment SET deleted_at = NOW() WHERE id = $1")
        .bind(c)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        comment_version(&pool, c).await,
        3,
        "論理削除も通常の更新として版が進む"
    );
    assert_eq!(
        ticket_version(&pool, ticket).await,
        v0 + 1,
        "論理削除ではコメント数は変わらないのでチケットは動かない"
    );

    sqlx::query("DELETE FROM tickets_comment WHERE id = $1")
        .bind(c)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        ticket_version(&pool, ticket).await,
        v0 + 2,
        "物理削除でもチケットの版が進む"
    );
    let (key, v): (Option<String>, i64) =
        sqlx::query_as("SELECT entity_key, sync_version FROM sync_tombstones WHERE entity = 'comment' AND entity_id = $1")
            .bind(c as i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((key, v), (Some(c.to_string()), 4));
}

#[tokio::test]
async fn comment_notify_carries_ticket_place_for_insert_and_delete() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (user, project, ticket) = setup(&pool).await;
    let team = team_of(&pool, ticket).await;
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("senn_sync").await.unwrap();

    let c = add_comment(&pool, ticket, user, "通知").await;
    // コメントの外部キーは連鎖削除ではない。チケットを消すときはコメントを先に消す（アプリと同じ順）
    sqlx::query("DELETE FROM tickets_comment WHERE id = $1")
        .bind(c)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tickets_ticket WHERE id = $1")
        .bind(ticket)
        .execute(&pool)
        .await
        .unwrap();

    let mut got = Vec::new();
    while let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(500), listener.recv()).await {
        got.push(serde_json::from_str::<serde_json::Value>(n.payload()).unwrap());
    }
    let mine: Vec<_> = got
        .iter()
        .filter(|p| p["e"] == "comment" && p["id"].as_i64() == Some(c as i64))
        .collect();
    assert_eq!(
        mine.iter()
            .map(|p| p["a"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["insert", "delete"]
    );
    for p in &mine {
        assert_eq!(p["t"], team);
        assert_eq!(p["p"], project);
        assert_eq!(p["tid"], ticket);
    }
    assert_eq!(mine[1]["v"], 2, "削除の版 = 直前の版 + 1");
}

#[tokio::test]
async fn comment_sync_api_respects_access_and_has_no_viewer_specific_fields() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (author, _, ticket) = setup(&pool).await;
    let team = team_of(&pool, ticket).await;
    let member = make_member(&pool, team).await;
    let outsider = create_test_user(&pool, "rt-outsider").await;
    let c = add_comment(&pool, ticket, author, "こんにちは").await;
    let cursor = None;

    use crate::infrastructure::repositories::sync_repo::sync_comments;
    let page = sync_comments(
        &pool,
        crate::infrastructure::repositories::sync_repo::ticket_access(&pool, member)
            .await
            .unwrap(),
        cursor.clone(),
        1000,
        "ai_agent",
    )
    .await
    .ok()
    .unwrap();
    let mine = page
        .changes
        .iter()
        .find(|x| x.id == c)
        .expect("メンバーには見える");
    assert_eq!(mine.ticket_id, ticket);
    assert_eq!(mine.body, "こんにちは");
    assert!(!mine.is_ai_agent_author);
    let json = serde_json::to_value(mine).unwrap();
    for k in ["canEdit", "canDelete", "replyCount"] {
        assert!(json.get(k).is_none(), "閲覧者に依存する値 {k} は持たない");
    }
    assert!(json["v"].as_i64().unwrap() >= 1);

    // チーム外の利用者には中身を渡さず、「見えなくなった」として返す
    let page = sync_comments(
        &pool,
        crate::infrastructure::repositories::sync_repo::ticket_access(&pool, outsider)
            .await
            .unwrap(),
        cursor,
        1000,
        "ai_agent",
    )
    .await
    .ok()
    .unwrap();
    assert!(
        page.changes.iter().all(|x| x.id != c),
        "チーム外には中身を渡さない"
    );
    assert!(page
        .deleted
        .iter()
        .any(|d| d.id == c as i64 && d.key.is_none()));

    // 論理削除: 本文は空
    sqlx::query("UPDATE tickets_comment SET deleted_at = NOW() WHERE id = $1")
        .bind(c)
        .execute(&pool)
        .await
        .unwrap();
    let page = sync_comments(
        &pool,
        crate::infrastructure::repositories::sync_repo::ticket_access(&pool, member)
            .await
            .unwrap(),
        None,
        1000,
        "ai_agent",
    )
    .await
    .ok()
    .unwrap();
    let mine = page.changes.iter().find(|x| x.id == c).unwrap();
    assert!(mine.is_deleted && mine.body.is_empty());
}

#[tokio::test]
async fn comment_sync_api_returns_hard_deletes_with_key_and_marks_ai_agent_author() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (author, _, ticket) = setup(&pool).await;
    let team = team_of(&pool, ticket).await;
    let member = make_member(&pool, team).await;
    let ai = sqlx::query_scalar::<_, String>("SELECT username FROM accounts_user WHERE id = $1")
        .bind(author)
        .fetch_one(&pool)
        .await
        .unwrap();
    let c = add_comment(&pool, ticket, author, "AI").await;

    use crate::infrastructure::repositories::sync_repo::sync_comments;
    let first = sync_comments(
        &pool,
        crate::infrastructure::repositories::sync_repo::ticket_access(&pool, member)
            .await
            .unwrap(),
        None,
        1000,
        &ai,
    )
    .await
    .ok()
    .unwrap();
    assert!(
        first
            .changes
            .iter()
            .find(|x| x.id == c)
            .unwrap()
            .is_ai_agent_author,
        "投稿者が AI エージェントのユーザー名と一致"
    );
    let cursor = crate::domain::models::sync_api::SyncCursor::decode(&first.cursor).ok();

    sqlx::query("DELETE FROM tickets_comment WHERE id = $1")
        .bind(c)
        .execute(&pool)
        .await
        .unwrap();
    let second = sync_comments(
        &pool,
        crate::infrastructure::repositories::sync_repo::ticket_access(&pool, member)
            .await
            .unwrap(),
        cursor,
        1000,
        &ai,
    )
    .await
    .ok()
    .unwrap();
    let del = second
        .deleted
        .iter()
        .find(|d| d.id == c as i64)
        .expect("物理削除は差分で返る");
    assert_eq!(
        del.key.as_deref(),
        Some(c.to_string().as_str()),
        "見えるので key 付き（= 削除。見えなくなっただけではない）"
    );
    assert!(del.v >= 2);
}

#[tokio::test]
async fn dispatch_pushes_comment_to_team_room_after_its_ticket_in_the_same_batch() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (user, _, ticket) = setup(&pool).await;
    let team = team_of(&pool, ticket).await;
    let other_team = create_test_team(&pool, "rt-other").await;
    let hub = Hub::new();
    let (tx_m, mut rx_m) = mpsc::channel(16);
    let (tx_o, mut rx_o) = mpsc::channel(16);
    hub.join(1, tx_m, vec![RoomId::team(team)]).await;
    hub.join(2, tx_o, vec![RoomId::team(other_team)]).await;

    let c = add_comment(&pool, ticket, user, "リアルタイム").await;
    let tv = ticket_version(&pool, ticket).await;
    let notices = vec![
        // 意図的にコメントを先に並べる（チケットが先に届くよう並べ直されること）
        Notice {
            e: "comment".into(),
            a: "insert".into(),
            id: c as i64,
            v: 1,
            t: Some(team),
            p: None,
            ot: None,
            opj: None,
            u: None,
            teams: vec![],
        },
        Notice {
            e: "ticket".into(),
            a: "update".into(),
            id: ticket as i64,
            v: tv,
            t: Some(team),
            p: None,
            ot: None,
            opj: None,
            u: None,
            teams: vec![],
        },
    ];
    dispatch(&pool, &hub, notices, "ai_agent").await.unwrap();

    let got = drain(&mut rx_m).await;
    assert_eq!(got.len(), 1);
    let changes = got[0]["changes"].as_array().unwrap();
    assert_eq!(
        changes
            .iter()
            .map(|c| c["entity"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["ticket", "comment"]
    );
    let cm = &changes[1];
    assert_eq!(cm["op"], "upsert");
    assert_eq!(cm["data"]["body"], "リアルタイム");
    assert_eq!(cm["data"]["ticketId"], ticket);
    assert_eq!(cm["data"]["author"]["id"], user);
    assert!(drain(&mut rx_o).await.is_empty(), "別チームには届かない");
}

#[tokio::test]
async fn moving_a_ticket_sends_its_comments_to_the_new_team() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (user, project, ticket) = setup(&pool).await;
    let old_team = team_of(&pool, ticket).await;
    let new_team = create_test_team(&pool, "rt-new").await;
    sqlx::query(
        "INSERT INTO tickets_project_teams (project_id, team_id, joined_at) VALUES ($1, $2, NOW())",
    )
    .bind(project)
    .bind(new_team)
    .execute(&pool)
    .await
    .unwrap();
    let c = add_comment(&pool, ticket, user, "移動前からあるコメント").await;
    let before = comment_version(&pool, c).await;

    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("senn_sync").await.unwrap();
    sqlx::query("UPDATE tickets_ticket SET team_id = $2 WHERE id = $1")
        .bind(ticket)
        .bind(new_team)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        comment_version(&pool, c).await,
        before + 1,
        "チケット移動でコメントの版も進む"
    );

    let mut got = Vec::new();
    while let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(500), listener.recv()).await {
        got.push(serde_json::from_str::<serde_json::Value>(n.payload()).unwrap());
    }
    let note = got
        .iter()
        .find(|p| p["e"] == "comment" && p["id"].as_i64() == Some(c as i64))
        .expect("コメントの通知");
    assert_eq!(note["t"], new_team, "移動先のチームとして通知される");
    assert_ne!(note["t"], old_team);
}

// ── 合図（Phase 3） ──

async fn collect_notices(listener: &mut PgListener) -> Vec<serde_json::Value> {
    let mut got = Vec::new();
    while let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(500), listener.recv()).await {
        got.push(serde_json::from_str::<serde_json::Value>(n.payload()).unwrap());
    }
    got
}

#[tokio::test]
async fn attachment_and_reaction_triggers_signal_with_the_ticket_place() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (user, project, ticket) = setup(&pool).await;
    let team = team_of(&pool, ticket).await;
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("senn_sync").await.unwrap();

    let att: i32 = sqlx::query_scalar(
        "INSERT INTO tickets_attachment (filename, file, file_size, created_at, uploader_id, ticket_id) VALUES ('a.txt', 'x/a.txt', 1, NOW(), $1, $2) RETURNING id::int4",
    )
    .bind(user).bind(ticket).fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO tickets_reaction (ticket_id, user_id, emoji_kind, emoji_value) VALUES ($1, $2, 'unicode', '👍')")
        .bind(ticket).bind(user).execute(&pool).await.unwrap();

    let got = collect_notices(&mut listener).await;
    let a = got
        .iter()
        .find(|p| p["e"] == "attachment" && p["id"].as_i64() == Some(att as i64))
        .expect("添付の合図");
    assert_eq!(
        (a["t"].as_i64(), a["p"].as_i64()),
        (Some(team as i64), Some(project as i64))
    );
    let r = got
        .iter()
        .find(|p| p["e"] == "reaction" && p["tid"].as_i64() == Some(ticket as i64))
        .expect("リアクションの合図");
    assert_eq!(
        r["id"].as_i64(),
        Some(ticket as i64),
        "リアクションの id はチケットの id"
    );
}

#[tokio::test]
async fn reaction_removed_by_ticket_deletion_cascade_sends_nothing() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (user, _, ticket) = setup(&pool).await;
    sqlx::query("INSERT INTO tickets_reaction (ticket_id, user_id, emoji_kind, emoji_value) VALUES ($1, $2, 'unicode', '🎉')")
        .bind(ticket).bind(user).execute(&pool).await.unwrap();
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("senn_sync").await.unwrap();

    sqlx::query("DELETE FROM tickets_ticket WHERE id = $1")
        .bind(ticket)
        .execute(&pool)
        .await
        .unwrap();
    let got = collect_notices(&mut listener).await;
    assert!(
        got.iter()
            .all(|p| !(p["e"] == "reaction" && p["tid"].as_i64() == Some(ticket as i64))),
        "チケット削除の連鎖で消えるリアクションは、親が無く部屋が決められないので送らない: {got:?}"
    );
}

#[tokio::test]
async fn cycle_wiki_and_notification_triggers_carry_their_scope() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let (user, project, _) = setup(&pool).await;
    let team: i32 = sqlx::query_scalar(
        "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 LIMIT 1",
    )
    .bind(project)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("senn_sync").await.unwrap();

    let cycle: i32 = sqlx::query_scalar(
        "INSERT INTO t_cycle (name, number, start_date, end_date, status, created_at, project_id, team_id) VALUES ('c', 1, CURRENT_DATE, CURRENT_DATE + 7, 'planned', NOW(), $1, $2) RETURNING id::int4",
    ).bind(project).bind(team).fetch_one(&pool).await.unwrap();
    let slug = format!("w{}", uuid::Uuid::new_v4().simple());
    let wiki: i32 = sqlx::query_scalar(
        "INSERT INTO wiki_page (title, slug, category, content, created_at, updated_at, author_id, project_id) VALUES ('t', $1, 'general', 'c', NOW(), NOW(), $2, $3) RETURNING id::int4",
    ).bind(&slug).bind(user).bind(project).fetch_one(&pool).await.unwrap();
    let team_slug = format!("m{}", uuid::Uuid::new_v4().simple());
    let team_wiki: i32 = sqlx::query_scalar(
        "INSERT INTO wiki_page (title, slug, category, content, created_at, updated_at, author_id, team_id) VALUES ('t', $1, 'general', 'c', NOW(), NOW(), $2, $3) RETURNING id::int4",
    ).bind(&team_slug).bind(user).bind(team).fetch_one(&pool).await.unwrap();
    let global_slug = format!("g{}", uuid::Uuid::new_v4().simple());
    let global_wiki: i32 = sqlx::query_scalar(
        "INSERT INTO wiki_page (title, slug, category, content, created_at, updated_at, author_id) VALUES ('t', $1, 'general', 'c', NOW(), NOW(), $2) RETURNING id::int4",
    ).bind(&global_slug).bind(user).fetch_one(&pool).await.unwrap();
    let notif: i32 = sqlx::query_scalar(
        "INSERT INTO notifications_notification (category, title, message, is_read, created_at, user_id) VALUES ('ticket', 't', 'm', false, NOW(), $1) RETURNING id::int4",
    ).bind(user).fetch_one(&pool).await.unwrap();

    let got = collect_notices(&mut listener).await;
    let find = |e: &str, id: i32| {
        got.iter()
            .find(|p| p["e"] == e && p["id"].as_i64() == Some(id as i64))
            .cloned()
    };
    let c = find("cycle", cycle).expect("サイクルの合図");
    assert_eq!(
        (c["t"].as_i64(), c["p"].as_i64()),
        (Some(team as i64), Some(project as i64)),
        "サイクルはチームに属する: {c}"
    );
    let w = find("wiki", wiki).expect("Wiki の合図");
    assert_eq!(w["p"].as_i64(), Some(project as i64));
    assert!(
        w["teams"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t.as_i64() == Some(team as i64)),
        "所属チームを添える: {w}"
    );
    let gw = find("wiki", global_wiki).expect("全体 Wiki の合図");
    assert!(
        gw["p"].is_null() && gw["t"].is_null(),
        "チームもプロジェクトも無い = 全員宛"
    );
    let tw = find("wiki", team_wiki).expect("チーム専用 Wiki の合図");
    assert_eq!(
        tw["t"].as_i64(),
        Some(team as i64),
        "チーム専用はそのチーム宛: {tw}"
    );
    let n = find("notification", notif).expect("通知の合図");
    assert_eq!(n["u"].as_i64(), Some(user as i64), "宛先は通知の持ち主");
}

#[tokio::test]
async fn dispatch_delivers_notification_signal_only_to_the_owner() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let hub = Hub::new();
    let (tx_owner, mut rx_owner) = mpsc::channel(16);
    let (tx_other, mut rx_other) = mpsc::channel(16);
    hub.join(
        5,
        tx_owner,
        vec![RoomId::user(5), super::rooms::global_room()],
    )
    .await;
    hub.join(
        6,
        tx_other,
        vec![RoomId::user(6), super::rooms::global_room()],
    )
    .await;

    let notice = Notice {
        e: "notification".into(),
        a: "insert".into(),
        id: 1,
        v: 0,
        t: None,
        p: None,
        ot: None,
        opj: None,
        u: Some(5),
        teams: vec![],
    };
    dispatch(&pool, &hub, vec![notice], "ai_agent")
        .await
        .unwrap();

    let got = drain(&mut rx_owner).await;
    assert_eq!(got[0]["room"], "u:5");
    assert_eq!(
        got[0]["changes"][0],
        serde_json::json!({"op": "stale", "entity": "notification", "id": 0, "v": 0})
    );
    assert!(
        drain(&mut rx_other).await.is_empty(),
        "他人の通知の合図は届かない"
    );
}

// ── 死活監視・計測（Phase 4） ──

use super::dispatcher::{spawn_with, Timing};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;

/// 目印（application_name）つきのプール。並行して動く他のテストの接続を巻き込まずに、この配信役の接続だけを切れる
async fn tagged_pool() -> Option<(PgPool, String)> {
    test_pool().await?; // スキーマの用意（DB が無ければ None）
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()?;
    let tag = format!("rt-{}", uuid::Uuid::new_v4().simple());
    let opts = PgConnectOptions::from_str(&url)
        .ok()?
        .application_name(&tag);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .ok()?;
    Some((pool, tag))
}

fn fast(ignore_pongs: bool) -> Timing {
    Timing {
        ping_interval: Duration::from_millis(100),
        ping_timeout: Duration::from_millis(600),
        log_interval: Duration::from_secs(3600),
        ignore_pongs,
    }
}

/// 実在のユーザーを、チームのメンバーにして、そのユーザーの本来の購読(今の判定の範囲)で Hub に入れる。
/// 公開区分の変更(並行する他のテストが行う)で 'all' の再計算が起きても、購読が変わらないようにするため。
async fn join_as_member(pool: &PgPool, hub: &Hub, tx: mpsc::Sender<Frame>, team: i32) -> i32 {
    let user = create_test_user(pool, "rt-mem").await;
    sqlx::query("INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1::int8, $2::int8, 'member', NOW())")
        .bind(team as i64)
        .bind(user as i64)
        .execute(pool)
        .await
        .unwrap();
    let access = crate::infrastructure::repositories::sync_repo::realtime_access(pool, user)
        .await
        .unwrap()
        .unwrap();
    let rooms = super::rooms::rooms_for_access(&access, user);
    hub.join(user, tx, rooms).await;
    user
}

#[tokio::test]
async fn ping_round_trips_and_a_healthy_listener_is_never_resynced() {
    let Some((pool, _tag)) = tagged_pool().await else {
        return;
    };
    let hub = Hub::new();
    // CI の実行機はデプロイと共用で、たまに 1 回だけ往復が数百ミリ秒かかる(2026-10-01 に 526ms で失敗)。
    // 「健全なら再同期しない」を確かめるテストなので、見切りの時間は本番と同じ長さにし、
    // 一時的な遅れを「死んだ」と誤判定させない(黙って死んだときの検知は別のテストで確かめる)。
    let timing = Timing {
        ping_timeout: Timing::default().ping_timeout,
        ..fast(false)
    };
    spawn_with(pool.clone(), hub.clone(), "ai_agent".into(), timing);
    tokio::time::sleep(Duration::from_millis(400)).await; // 配信役が LISTEN を始めるのを待つ
    let (tx, mut rx) = mpsc::channel(64);
    hub.join(1, tx, vec![RoomId::team(1)]).await;

    tokio::time::sleep(Duration::from_millis(1500)).await;
    let stats = hub.snapshot().await;
    assert!(
        stats.ping_roundtrip.count >= 5,
        "ping が往復している: {:?}",
        stats.ping_roundtrip
    );
    // 1 回の遅れでは落とさず、平均で速さを見る。最大値は、明らかな詰まりだけを拾う緩い上限にする
    assert!(
        stats.ping_roundtrip.avg_ms < 200.0,
        "往復は平均して速い: {:?}",
        stats.ping_roundtrip
    );
    assert!(
        stats.ping_roundtrip.max_ms < 2000,
        "往復が詰まっていない: {:?}",
        stats.ping_roundtrip
    );
    assert_eq!(stats.totals.watchdog_timeouts, 0);
    let got = drain(&mut rx).await;
    assert!(
        got.iter().all(|p| p["type"] != "resync"),
        "健全なのに再同期が送られた: {got:?}"
    );
}

#[tokio::test]
async fn silent_listener_death_is_detected_then_everyone_is_resynced_and_the_listener_is_rebuilt() {
    let Some((pool, _tag)) = tagged_pool().await else {
        return;
    };
    let hub = Hub::new();
    // 返事を無視 = 接続は生きているのに、通知が戻ってこない（半開きの接続と同じ見え方）
    spawn_with(pool.clone(), hub.clone(), "ai_agent".into(), fast(true));
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (tx, mut rx) = mpsc::channel(64);
    let team = create_test_team(&pool, "rt-silent").await;
    join_as_member(&pool, &hub, tx, team).await;

    for _ in 0..40 {
        if hub.stats.totals().watchdog_timeouts >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        hub.stats.totals().watchdog_timeouts >= 1,
        "返事が戻らないので、死んでいると判断する"
    );
    let got = drain(&mut rx).await;
    let resync = got
        .iter()
        .find(|p| p["type"] == "resync" && p["room"] == format!("t:{team}"))
        .expect("全部屋に再同期が送られる");
    assert_eq!(resync["reason"], "epoch_changed");
}

#[tokio::test]
async fn dispatcher_recovers_when_its_listen_connection_is_killed_and_keeps_delivering() {
    let Some((pool, tag)) = tagged_pool().await else {
        return;
    };
    let (user, project, ticket) = setup(&pool).await;
    let team = team_of(&pool, ticket).await;
    let hub = Hub::new();
    spawn_with(pool.clone(), hub.clone(), "ai_agent".into(), fast(false));
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (tx, mut rx) = mpsc::channel(64);
    join_as_member(&pool, &hub, tx, team).await;
    let _ = user;
    let _ = project;

    // LISTEN の接続だけを強制的に切る（この配信役の application_name のものだけ）
    let killed: i64 = sqlx::query_scalar(
        "SELECT count(pg_terminate_backend(pid)) FROM pg_stat_activity WHERE application_name = $1 AND query LIKE 'LISTEN%' AND pid <> pg_backend_pid()",
    )
    .bind(&tag)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(killed >= 1, "LISTEN の接続を切れた: {killed}");

    // 切断を検知して全部屋に再同期が送られる
    for _ in 0..40 {
        if hub.stats.totals().listener_lost >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(hub.stats.totals().listener_lost >= 1);
    let got = drain(&mut rx).await;
    assert!(
        got.iter().any(|p| p["type"] == "resync"),
        "切断中の通知は失われているので、再同期を求める: {got:?}"
    );

    // 復旧後も、更新が配信される
    tokio::time::sleep(Duration::from_millis(500)).await;
    sqlx::query("UPDATE tickets_ticket SET title = '復旧後の更新' WHERE id = $1")
        .bind(ticket)
        .execute(&pool)
        .await
        .unwrap();
    let mut delivered = false;
    for _ in 0..30 {
        let got = drain(&mut rx).await;
        if got
            .iter()
            .any(|p| p["type"] == "delta" && p["changes"][0]["data"]["title"] == "復旧後の更新")
        {
            delivered = true;
            break;
        }
    }
    assert!(delivered, "接続が復旧して、更新が届く");
}

/// アクセス制御の再設計 E-3: 役割・有効性の変更はそのユーザー、公開区分の変更は 'all' を senn_access に通知する
#[tokio::test]
async fn role_and_visibility_changes_notify_access_recompute() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let user = create_test_user(&pool, "rt-acc").await;
    let team = create_test_team(&pool, "rt-acc").await;
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("senn_access").await.unwrap();

    // 役割に関係の無い列の変更では通知しない(他のテストの通知と区別するため、ユーザー ID で見る)
    sqlx::query("UPDATE accounts_user SET display_name = 'x' WHERE id = $1::int8")
        .bind(user as i64)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE accounts_user SET is_guest = true WHERE id = $1::int8")
        .bind(user as i64)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = $1::int8")
        .bind(team as i64)
        .execute(&pool)
        .await
        .unwrap();

    let mut payloads = Vec::new();
    while let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(500), listener.recv()).await {
        payloads.push(n.payload().to_string());
    }
    let mine = payloads.iter().filter(|p| **p == user.to_string()).count();
    assert_eq!(
        mine, 1,
        "is_guest の変更で 1 回だけ(display_name では送らない): {payloads:?}"
    );
    assert!(
        payloads.iter().any(|p| p == "all"),
        "公開区分の変更で 'all': {payloads:?}"
    );
}
