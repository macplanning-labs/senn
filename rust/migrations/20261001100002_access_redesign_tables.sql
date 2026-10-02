-- アクセス制御の再設計: 監査記録と、試運転の差分(docs/design/詳細設計書_アクセス制御_再設計.md §3.2)

-- 監査記録: 孤立チームの救済・公開区分の変更・招待・Owner の指名・キーの操作など
CREATE TABLE IF NOT EXISTS public.access_audit_log (
    id             bigserial PRIMARY KEY,
    actor_user_id  bigint REFERENCES public.accounts_user(id) ON DELETE SET NULL,  -- 連携(Integration)は NULL
    actor_kind     text NOT NULL CHECK (actor_kind IN ('human', 'personal_key', 'integration', 'system')),
    action         text NOT NULL,
    team_id        bigint,
    target_user_id bigint,
    detail         jsonb NOT NULL DEFAULT '{}'::jsonb,  -- 秘密情報(トークン・Webhook URL 等)は入れない
    created_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS access_audit_log_team_idx ON public.access_audit_log (team_id, created_at DESC);

-- 試運転(shadow)の差分: 新しい判定と今の判定が違った記録。本文は入れない(種類と ID だけ)。
-- フェーズ H(有効化)の後に削除する
CREATE TABLE IF NOT EXISTS public.access_shadow_diff (
    id          bigserial PRIMARY KEY,
    resource    text NOT NULL,
    direction   text NOT NULL CHECK (direction IN ('newly_hidden', 'newly_visible')),
    user_id     bigint NOT NULL,
    resource_id bigint,
    route       text,
    occurred_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS access_shadow_diff_idx ON public.access_shadow_diff (resource, direction, occurred_at DESC);
