-- アクセス制御の再設計 フェーズ A-1(詳細設計書 §3.3)
-- 招待(社外の人・Guest の入口)と、社内ドメインの自己登録のメール確認。
-- トークンは SHA-256 の16進で保存し、生のトークンは保存しない(1 回限り・期限つき)。
-- accounts_user.id は bigint のため、参照する列も bigint にする(設計書の integer から修正)。
-- 再実行しても失敗しない形にする(IF NOT EXISTS)。

CREATE TABLE IF NOT EXISTS public.access_invitation (
  id                bigserial PRIMARY KEY,
  email             text NOT NULL,
  token_hash        text NOT NULL UNIQUE,
  role              text NOT NULL CHECK (role IN ('full_member', 'guest')),
  team_id           bigint REFERENCES public.m_team(id) ON DELETE CASCADE,
  scoped_project_id bigint REFERENCES public.tickets_project(id) ON DELETE CASCADE,
  end_date          date,
  invited_by        bigint NOT NULL REFERENCES public.accounts_user(id),
  expires_at        timestamptz NOT NULL,
  used_at           timestamptz,
  revoked_at        timestamptz,
  created_at        timestamptz NOT NULL DEFAULT now(),
  CHECK (role = 'full_member' OR team_id IS NOT NULL)   -- Guest は招待先(チーム)が必須
);
CREATE INDEX IF NOT EXISTS access_invitation_team_idx ON public.access_invitation (team_id);
CREATE INDEX IF NOT EXISTS access_invitation_email_idx ON public.access_invitation (lower(email));

CREATE TABLE IF NOT EXISTS public.access_email_verification (
  id          bigserial PRIMARY KEY,
  user_id     bigint NOT NULL REFERENCES public.accounts_user(id) ON DELETE CASCADE,
  token_hash  text NOT NULL UNIQUE,
  expires_at  timestamptz NOT NULL,
  used_at     timestamptz,
  created_at  timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE public.accounts_user ADD COLUMN IF NOT EXISTS email_verified_at timestamptz;
