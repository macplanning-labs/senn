/// infrastructure/repositories/user_ai_prompt_template_repo.rs — ユーザーごとの AI プロンプトテンプレート永続化
use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserAiPromptTemplateRow {
    pub common_template: Option<String>,
    pub cursor_template: Option<String>,
    pub claude_template: Option<String>,
}

/// ユーザーの AI プロンプトテンプレートを取得する。
/// 行が存在しない場合は全て None を返す。
pub async fn get_by_user(pool: &PgPool, user_id: i32) -> anyhow::Result<UserAiPromptTemplateRow> {
    let row_opt: Option<UserAiPromptTemplateRow> = sqlx::query_as(
        "SELECT common_template, cursor_template, claude_template FROM accounts_user_ai_prompt_template WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row_opt {
        Ok(row)
    } else {
        // 行が無い場合は全て None で返す
        Ok(UserAiPromptTemplateRow {
            common_template: None,
            cursor_template: None,
            claude_template: None,
        })
    }
}

/// ユーザーの AI プロンプトテンプレートを作成/更新する(UPSERT)。
/// 値は前後の空白を trim し、空白文字列は NULL として保存される。
pub async fn upsert(
    pool: &PgPool,
    user_id: i32,
    common_template: Option<String>,
    cursor_template: Option<String>,
    claude_template: Option<String>,
) -> anyhow::Result<()> {
    // 空白のみの文字列を NULL に変換
    let common = common_template
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let cursor = cursor_template
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let claude = claude_template
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    sqlx::query(
        "INSERT INTO accounts_user_ai_prompt_template (user_id, common_template, cursor_template, claude_template)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO UPDATE SET
           common_template = excluded.common_template,
           cursor_template = excluded.cursor_template,
           claude_template = excluded.claude_template,
           updated_at = NOW()"
    )
    .bind(user_id)
    .bind(common)
    .bind(cursor)
    .bind(claude)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    /// 未設定の場合: 全て None を返す
    #[tokio::test]
    async fn get_by_user_returns_all_none_when_not_set() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user_id = test_support::create_test_user(&pool, "ai_prompt_test").await;
        let row = get_by_user(&pool, user_id).await.unwrap();
        assert_eq!(row.common_template, None);
        assert_eq!(row.cursor_template, None);
        assert_eq!(row.claude_template, None);
    }

    /// PUT→GET往復: 保存した値が取得できる
    #[tokio::test]
    async fn upsert_and_get_returns_same_values() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user_id = test_support::create_test_user(&pool, "ai_prompt_upsert").await;

        let common = Some("Common rules".to_string());
        let cursor = Some("Cursor rules".to_string());
        let claude = Some("Claude rules".to_string());

        upsert(
            &pool,
            user_id,
            common.clone(),
            cursor.clone(),
            claude.clone(),
        )
        .await
        .unwrap();

        let row = get_by_user(&pool, user_id).await.unwrap();
        assert_eq!(row.common_template, common);
        assert_eq!(row.cursor_template, cursor);
        assert_eq!(row.claude_template, claude);
    }

    /// null/空白でクリア: 空白文字列は NULL として保存される
    #[tokio::test]
    async fn upsert_clears_empty_and_whitespace() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user_id = test_support::create_test_user(&pool, "ai_prompt_clear").await;

        // 最初に値を設定
        upsert(
            &pool,
            user_id,
            Some("Initial common".to_string()),
            Some("Initial cursor".to_string()),
            Some("Initial claude".to_string()),
        )
        .await
        .unwrap();

        // 空白・None で更新
        upsert(
            &pool,
            user_id,
            Some("   ".to_string()), // 空白のみ
            None,                    // None
            Some("".to_string()),    // 空文字列
        )
        .await
        .unwrap();

        let row = get_by_user(&pool, user_id).await.unwrap();
        assert_eq!(row.common_template, None);
        assert_eq!(row.cursor_template, None);
        assert_eq!(row.claude_template, None);
    }

    /// 前後の空白が trim される
    #[tokio::test]
    async fn upsert_trims_whitespace() {
        let Some(pool) = test_support::test_pool().await else {
            return;
        };
        let user_id = test_support::create_test_user(&pool, "ai_prompt_trim").await;

        upsert(
            &pool,
            user_id,
            Some("  common  ".to_string()),
            Some("\tcursor\n".to_string()),
            Some("  claude  ".to_string()),
        )
        .await
        .unwrap();

        let row = get_by_user(&pool, user_id).await.unwrap();
        assert_eq!(row.common_template, Some("common".to_string()));
        assert_eq!(row.cursor_template, Some("cursor".to_string()));
        assert_eq!(row.claude_template, Some("claude".to_string()));
    }
}
