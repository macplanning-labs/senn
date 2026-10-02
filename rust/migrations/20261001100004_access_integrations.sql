-- アクセス制御の再設計 フェーズ F-7(詳細設計書 §3.4)
-- 外部連携(Integration)。人の権限を借りず、許可したチームだけで動く。
-- キーは SHA-256 の16進(ai_agent_api_keys と同じ形)で保存し、生のキーは保存しない。
-- 再実行しても失敗しない形にする(IF NOT EXISTS)。

CREATE TABLE IF NOT EXISTS public.access_integration (
  id          bigserial PRIMARY KEY,
  name        text NOT NULL,
  key_hash    text NOT NULL UNIQUE,
  created_by  bigint NOT NULL REFERENCES public.accounts_user(id),
  is_active   boolean NOT NULL DEFAULT true,
  created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS public.access_integration_team (
  integration_id bigint NOT NULL REFERENCES public.access_integration(id) ON DELETE CASCADE,
  team_id        bigint NOT NULL REFERENCES public.m_team(id) ON DELETE CASCADE,
  PRIMARY KEY (integration_id, team_id)
);
