-- AI Agent を利用した MCP 操作: 端末の接続(コピー・貼り付けなし)(DEMO-000192 フェーズ4)
-- 利用者の Mac の MCP が、自分でキーを作り、「指紋(SHA-256)」だけをサーバーに渡す。
-- 利用者が画面で申請し、管理者が承認すると、預かった指紋で ai_agent_api_keys に自動登録する。
-- キーの平文は、Mac の外に出ない(このテーブルにも保存しない)。
-- 「期限切れ」は状態にせず、waiting のまま expires_at を過ぎたものとして扱う。
CREATE TABLE IF NOT EXISTS public.ai_mcp_pairing (
    id                 bigserial PRIMARY KEY,
    code               text NOT NULL UNIQUE,
    device_secret_hash text NOT NULL,
    key_hash           text NOT NULL UNIQUE,
    key_prefix         text NOT NULL,
    label              text NOT NULL DEFAULT '',
    status             text NOT NULL CHECK (status IN ('waiting', 'requested', 'approved', 'rejected', 'failed')),
    message            text,
    user_id            bigint REFERENCES public.accounts_user(id) ON DELETE CASCADE,
    request_id         bigint REFERENCES public.ai_mcp_access_request(id) ON DELETE SET NULL,
    key_id             bigint REFERENCES public.ai_agent_api_keys(id) ON DELETE SET NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    expires_at         timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS ai_mcp_pairing_request_idx ON public.ai_mcp_pairing (request_id);
CREATE INDEX IF NOT EXISTS ai_mcp_pairing_created_idx ON public.ai_mcp_pairing (created_at DESC);
