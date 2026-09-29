-- User-level AI prompt templates (common, Cursor, Claude Code)
-- Move おまじない from project-level to user-level settings

CREATE TABLE IF NOT EXISTS accounts_user_ai_prompt_template (
    user_id integer PRIMARY KEY REFERENCES accounts_user(id) ON DELETE CASCADE,
    common_template text,
    cursor_template text,
    claude_template text,
    updated_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE accounts_user_ai_prompt_template IS 'ユーザーごとの AI プロンプトテンプレート(おまじない)。common/cursor/claude の3ツール別。';
COMMENT ON COLUMN accounts_user_ai_prompt_template.common_template IS 'デフォルトのおまじない(ツール指定なし時)';
COMMENT ON COLUMN accounts_user_ai_prompt_template.cursor_template IS 'Cursor専用のおまじない';
COMMENT ON COLUMN accounts_user_ai_prompt_template.claude_template IS 'Claude Code専用のおまじない';

-- Drop unused cursor/claude columns from project-level (20260929100001 で追加された列を削除)
ALTER TABLE tickets_project DROP COLUMN IF EXISTS ai_prompt_template_cursor;
ALTER TABLE tickets_project DROP COLUMN IF EXISTS ai_prompt_template_claude;

-- Clear existing cached prompts (旧プロジェクトおまじないが埋め込まれているため)
UPDATE tickets_ticket SET ai_prompt = NULL, ai_prompt_updated_at = NULL, ai_prompt_generation_mode = NULL WHERE ai_prompt IS NOT NULL;
