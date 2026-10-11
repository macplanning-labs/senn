-- AI Agent を利用した MCP 操作の申請: 再通知の履歴(DEMO-000192 フェーズ1b)
-- 3日以上 pending のままの申請に対して、1回だけ再通知を送る仕組み。
-- 再通知済みかどうかを reminded_at で判定する。
ALTER TABLE public.ai_mcp_access_request ADD COLUMN IF NOT EXISTS reminded_at timestamptz;
