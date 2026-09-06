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

/// テスト用DBへの接続プールを取得する。
///
/// `TEST_DATABASE_URL` を優先し、無ければ `DATABASE_URL` を使う。
/// どちらも未設定、または接続に失敗した場合は `None` を返す
/// (テストをpanicさせず、呼び出し側で早期returnしてスキップできるようにする。
/// テストDBが用意されていない環境でも `cargo test` 全体は壊さない)。
pub async fn test_pool() -> Option<PgPool> {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()?;
    match PgPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!("[test_support] テストDBへの接続に失敗したためスキップします: {e:?}");
            None
        }
    }
}

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

/// 使い捨てのテスト用プロジェクトを作成し、その `id` を返す。
/// (`tickets_project` に author_id は存在しない。呼び出し側の引数は
/// 他のヘルパーとシグネチャを揃えるため受け取るが未使用)
pub async fn create_test_project(pool: &PgPool, prefix_base: &str, _author_id: i32) -> i32 {
    let prefix = format!("{prefix_base}{}", unique_suffix());
    let prefix = &prefix[..prefix.len().min(20)];
    sqlx::query_scalar::<_, i32>(
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
    .expect("テストプロジェクト作成に失敗")
}

/// 使い捨てのテスト用チケットを作成し、その `id` を返す。
pub async fn create_test_ticket(pool: &PgPool, project_id: i32, key_prefix: &str, author_id: i32) -> i32 {
    let ticket_key = format!("{key_prefix}-{}", unique_suffix());
    sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO tickets_ticket
            (ticket_key, title, description, status, priority, ticket_type,
             project_id, author_id, created_at, updated_at, gantt_order)
        VALUES
            ($1, $1, '', 'open', 'medium', 'task',
             $2, $3, NOW(), NOW(), 0)
        RETURNING id::int4
        "#,
    )
    .bind(&ticket_key)
    .bind(project_id)
    .bind(author_id)
    .fetch_one(pool)
    .await
    .expect("テストチケット作成に失敗")
}
