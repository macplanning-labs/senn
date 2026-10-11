-- AI Agent を利用した MCP 操作の申請(DEMO-000192)
-- 利用者が申請し、システム管理者が承認すると、本人が自分専用のAIキー(ai_agent_api_keys)を発行できる。
-- キー本体は承認時には作らない(本人が発行する時に作り、その場で一度だけ表示する)。
-- 「期限切れ」は状態として持たず、承認から approval_expires_at を過ぎたかで判定する。
CREATE TABLE IF NOT EXISTS public.ai_mcp_access_request (
    id                  bigserial PRIMARY KEY,
    user_id             bigint NOT NULL REFERENCES public.accounts_user(id) ON DELETE CASCADE,
    purpose             text NOT NULL DEFAULT '',
    status              text NOT NULL CHECK (status IN ('pending', 'approved', 'rejected', 'issued', 'cancelled')),
    decided_by          bigint REFERENCES public.accounts_user(id) ON DELETE SET NULL,
    decided_at          timestamptz,
    decision_note       text NOT NULL DEFAULT '',
    approval_expires_at timestamptz,
    issued_key_id       bigint REFERENCES public.ai_agent_api_keys(id) ON DELETE SET NULL,
    issued_at           timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now()
);

-- 1人につき、申請中・承認済み(未発行)の申請は同時に1件まで
CREATE UNIQUE INDEX IF NOT EXISTS ai_mcp_access_request_one_open_per_user
    ON public.ai_mcp_access_request (user_id)
    WHERE status IN ('pending', 'approved');

CREATE INDEX IF NOT EXISTS ai_mcp_access_request_status_idx
    ON public.ai_mcp_access_request (status, created_at DESC);
