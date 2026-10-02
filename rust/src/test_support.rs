/// test_support.rs — リポジトリ層テスト用の共通ヘルパー
///
/// このクレートはバイナリのみ(lib.rsが無い)ため、`rust/tests/`配下の
/// 結合テストからは内部モジュールにアクセスできない。そのため各リポジトリ
/// ファイル内に `#[cfg(test)] mod tests` を置き、本モジュール(`#[cfg(test)]`
/// のみでコンパイルされる)が提供する共通ヘルパーを使う方式にしている。
///
/// リポジトリ層の関数の多くは `pool: &PgPool` を直接受け取るシグネチャで
/// あり、Transactionを汎用的に受け取れる設計にはなっていない(この変更は
/// 影響範囲が大きいため今回のスコープ外)。そのためTransaction+rollbackで
/// はなく、開発標準書§7.4チェック8の代替方針「テストデータのIDは動的に
/// 生成する」を採用する: 各テストはUUID由来のユニークな接尾辞を持つ
/// username/ticket_key/prefixを使い、UNIQUE制約に抵触しないようにする。
/// 作成した行はテスト用DB専用(実データベースには接続しない)であり、
/// 明示的な削除は行わない(使い捨てのテストDBのため蓄積しても実害がない)。
use sqlx::PgPool;
use uuid::Uuid;

/// REQUIRE_TEST_DB の値が「必須」を意味するか判定する純関数
///
/// None / Some("") / Some("0") / Some("false")（大文字小文字問わず）→ false
/// それ以外 → true
pub(crate) fn require_db_flag(value: Option<&str>) -> bool {
    match value {
        None | Some("") => false,
        Some(s) => {
            let lower = s.to_lowercase();
            lower != "0" && lower != "false"
        }
    }
}

// DB初期化はプロセスごとに1回だけ行い、完了するまで他のテストを待たせる
// (フラグだけを立てて先へ進むと、初期化の途中で他のテストが DB を使い始めて失敗する)。
static DB_BOOTSTRAP: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

/// テスト用DBへの接続プールを取得する。
///
/// `TEST_DATABASE_URL` を優先し、無ければ `DATABASE_URL` を使う。
///
/// DBが使えない場合(未設定、または接続に失敗):
/// - `REQUIRE_TEST_DB` が設定されていて、値が空文字・0・false以外なら panic(スキップさせない)
/// - そうでなければ None を返す。**DBを使うテストは何も検証せず成功扱いになる**ので、警告を必ず出す
///
/// DBが使える場合:
/// - 空のDB(accounts_user テーブルがない)なら sqlx::migrate!() で初期化する
/// - スキーマがあるDBには何もしない(開発用DBのマイグレーション状態を変えない)
pub async fn test_pool() -> Option<PgPool> {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok();

    let unavailable = match url {
        None => "TEST_DATABASE_URL も DATABASE_URL も設定されていません".to_string(),
        Some(url) => match PgPool::connect(&url).await {
            Ok(pool) => {
                DB_BOOTSTRAP
                    .get_or_init(|| async {
                        let has_schema: bool = sqlx::query_scalar(
                            "SELECT to_regclass('public.accounts_user') IS NOT NULL",
                        )
                        .fetch_one(&pool)
                        .await
                        .expect("[test_support] テスト用DBのスキーマ確認に失敗しました");
                        if !has_schema {
                            sqlx::migrate!()
                                .run(&pool)
                                .await
                                .expect("[test_support] 空のテスト用DBの初期化(マイグレーション)に失敗しました");
                        }
                    })
                    .await;
                return Some(pool);
            }
            Err(e) => format!("テスト用DBに接続できません: {e}"),
        },
    };

    if require_db_flag(std::env::var("REQUIRE_TEST_DB").ok().as_deref()) {
        panic!("[test_support] REQUIRE_TEST_DB が設定されていますが、テスト用DBを使えません({unavailable})");
    }
    eprintln!(
        "[test_support] ⚠ テスト用DBが使えないため、DBを使うテストをスキップします(成功扱いになります): {unavailable}"
    );
    None
}

/// DB のセッションと同じ暦の「今日」(UTC)を返す。
///
/// sqlx は接続のたびにセッションのタイムゾーンを UTC に設定するため、SQL の `CURRENT_DATE` / `NOW()::date`
/// は UTC の日付になる。テストが `chrono::Local`(日本時間)で日付を作ると、日本時間の0時〜9時に
/// DB と日付が1日ずれて、日付を使う判定(期限・有効化など)のテストが落ちる。DB と比較する日付は、これで作る。
pub fn db_today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}

/// サイクルのテストを直列にするためのロック。
///
/// `auto_activate_due_cycles` / `auto_complete_overdue_cycles` は**全プロジェクト**のサイクルを対象に動く。
/// これらを呼ぶテストと、対象になりうる(期限が過ぎた)サイクルを作るテストが並列に走ると、
/// 互いのサイクルを先に有効化・完了してしまい、戻り値の不一致や、番号の重複
/// (`unique_cycle_number_per_project`)で間欠的に失敗する。cycle_repo のテストは、これを取ってから始める。
pub static CYCLE_GLOBAL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// テスト実行ごとに一意な短い接尾辞を生成する(UNIQUE制約回避用)。
pub fn unique_suffix() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}

/// 使い捨てのテスト用ユーザーを作成し、その `id` を返す。
/// `accounts_user` はNOT NULL制約が多いため、テストに必要な最小限の値のみ埋める。
pub async fn create_test_user(pool: &PgPool, username_prefix: &str) -> i32 {
    let username = format!("{username_prefix}-{}", unique_suffix());
    sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO accounts_user
            (password, is_superuser, username, first_name, last_name, email,
             is_staff, is_active, date_joined, display_name,
             must_change_password, email_notifications_enabled)
        VALUES
            ('!', false, $1, '', '', $1 || '@test.local',
             false, true, NOW(), $1,
             false, false)
        RETURNING id::int4
        "#,
    )
    .bind(&username)
    .fetch_one(pool)
    .await
    .expect("テストユーザー作成に失敗")
}

/// 使い捨てのテスト用チームを作成し、その `id` を返す。
pub async fn create_test_team(pool: &PgPool, name_prefix: &str) -> i32 {
    let suffix = unique_suffix();
    let slug = format!("t{suffix}");
    let prefix = format!("T{}", &suffix[..6.min(suffix.len())])
        .chars()
        .take(20)
        .collect::<String>();
    sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO m_team (name, slug, description, icon, color, slack_webhook_url, is_active, prefix, created_at)
        VALUES ($1, $2, '', '', '#6366f1', '', true, $3, NOW())
        RETURNING id::int4
        "#,
    )
    .bind(format!("{name_prefix}-{suffix}"))
    .bind(&slug)
    .bind(&prefix)
    .fetch_one(pool)
    .await
    .expect("テストチーム作成に失敗")
}

/// 使い捨てのテスト用プロジェクトを作成し、その `id` を返す。
/// (`tickets_project` に author_id は存在しない。呼び出し側の引数は
/// 他のヘルパーとシグネチャを揃えるため受け取るが未使用)
pub async fn create_test_project(pool: &PgPool, prefix_base: &str, _author_id: i32) -> i32 {
    let prefix = format!("{prefix_base}{}", unique_suffix());
    let prefix = &prefix[..prefix.len().min(20)];
    let slug = format!("t{}", unique_suffix());
    let team_id: i32 = sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO m_team (name, slug, description, icon, color, slack_webhook_url, is_active, prefix, created_at)
        VALUES ($1, $2, '', '', '#6366f1', '', true, $3, NOW())
        RETURNING id::int4
        "#,
    )
    .bind(format!("テストチーム-{prefix}"))
    .bind(&slug)
    .bind(prefix)
    .fetch_one(pool)
    .await
    .expect("テストチーム作成に失敗");

    let project_id = sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO tickets_project (name, prefix, description, status, created_at, grace_period_days)
        VALUES ($1, $2, '', 'active', NOW(), 0)
        RETURNING id::int4
        "#,
    )
    .bind(format!("テストプロジェクト-{prefix}"))
    .bind(prefix)
    .fetch_one(pool)
    .await
    .expect("テストプロジェクト作成に失敗");

    // Add team to project
    sqlx::query(
        "INSERT INTO tickets_project_teams (project_id, team_id, joined_at) VALUES ($1, $2, NOW())",
    )
    .bind(project_id)
    .bind(team_id)
    .execute(pool)
    .await
    .expect("テストプロジェクトへのチーム追加に失敗");

    project_id
}

/// 使い捨てのテスト用チケットを作成し、その `id` を返す。
pub async fn create_test_ticket(
    pool: &PgPool,
    project_id: i32,
    key_prefix: &str,
    author_id: i32,
) -> i32 {
    let ticket_key = format!("{key_prefix}-{}", unique_suffix());
    let team_id: i32 = sqlx::query_scalar(
        "SELECT team_id::int4 FROM tickets_project_teams WHERE project_id = $1 ORDER BY team_id LIMIT 1"
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .expect("テストチケット用の参加チーム取得に失敗");

    sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO tickets_ticket
            (ticket_key, title, description, status, priority, ticket_type,
             project_id, author_id, team_id, created_at, updated_at, gantt_order)
        VALUES
            ($1, $1, '', 'open', 'medium', 'task',
             $2, $3, $4, NOW(), NOW(), 0)
        RETURNING id::int4
        "#,
    )
    .bind(&ticket_key)
    .bind(project_id)
    .bind(author_id)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .expect("テストチケット作成に失敗")
}

/// ハンドラを直接呼ぶテスト用の設定(固定値。SMTP は無し = メールは送らない)
pub fn test_config() -> crate::config::AppConfig {
    crate::config::AppConfig {
        database_url: String::new(),
        port: 0,
        base_url: "https://senn.test".to_string(),
        additional_allowed_origins: vec!["https://stg.senn.test".to_string()],
        cookie_secure: true,
        media_dir: "media".to_string(),
        max_upload_size: 10_485_760,
        smtp_host: None,
        smtp_port: None,
        smtp_user: None,
        smtp_password: None,
        webauthn_rp_id: "senn.test".to_string(),
        webauthn_rp_origin: "https://senn.test".to_string(),
        jwt_secret: "test-secret-test-secret-test-secret".to_string(),
        access_token_lifetime_minutes: 30,
        refresh_token_lifetime_days: 7,
        mfa_token_lifetime_seconds: 300,
        password_reset_token_ttl_hours: 24,
        wip_api_key: None,
        wip_api_user: "管理者".to_string(),
        wip_ai_api_key: None,
        wip_ai_api_user: "ai_agent".to_string(),
        ollama_url: String::new(),
        ollama_model: String::new(),
        ollama_timeout_secs: 1,
        openai_api_key: None,
        openai_model: String::new(),
        openai_timeout_secs: 1,
        app_channel: "internal".to_string(),
    }
}

/// ハンドラを直接呼ぶテスト用の AppState
pub async fn test_state(pool: &PgPool) -> crate::presentation::state::AppState {
    crate::presentation::state::AppState::new(pool.clone(), test_config(), None)
        .await
        .expect("テスト用 AppState の作成に失敗")
}

/// ログインした人(ブラウザ)としての閲覧者を読み込む
pub async fn test_viewer(pool: &PgPool, user_id: i32) -> crate::domain::access::Viewer {
    crate::infrastructure::access::viewer_repo::load(
        pool,
        crate::domain::access::Principal::Human { user_id },
        crate::infrastructure::access::viewer_repo::today_utc(),
    )
    .await
    .expect("閲覧者の読み込みに失敗")
    .expect("閲覧者が見つからない")
}

/// チーム全体の所属を追加する(role: 'admin' = Owner / 'member')
pub async fn add_test_team_member(pool: &PgPool, team_id: i32, user_id: i32, role: &str) {
    sqlx::query(
        "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1::int8, $2::int8, $3, NOW())",
    )
    .bind(team_id as i64)
    .bind(user_id as i64)
    .bind(role)
    .execute(pool)
    .await
    .expect("テスト用のチーム所属の追加に失敗");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_db_flag_parses_correctly() {
        // false と判定される値
        assert!(!require_db_flag(None));
        assert!(!require_db_flag(Some("")));
        assert!(!require_db_flag(Some("0")));
        assert!(!require_db_flag(Some("false")));
        assert!(!require_db_flag(Some("FALSE")));
        assert!(!require_db_flag(Some("False")));

        // true と判定される値
        assert!(require_db_flag(Some("1")));
        assert!(require_db_flag(Some("true")));
        assert!(require_db_flag(Some("TRUE")));
        assert!(require_db_flag(Some("True")));
        assert!(require_db_flag(Some("yes")));
        assert!(require_db_flag(Some("any other value")));
    }
}
