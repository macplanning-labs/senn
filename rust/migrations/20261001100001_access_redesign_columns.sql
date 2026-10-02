-- アクセス制御の再設計: 列の追加(docs/design/詳細設計書_アクセス制御_再設計.md §3.1)
-- 追加だけ。既存の動作は変えない(判定はフェーズ H まで試運転)。
-- - m_team.visibility        : 'public' | 'private'(既存チームはすべて public)
-- - m_team.settings_policy   : 'members' | 'owners'(チームの設定を、メンバー全員 / Owner だけが管理する)
-- - accounts_user.is_guest   : Guest(招待されたチーム・プロジェクトの外は見えない)
-- - accounts_user.is_system_admin : is_staff の後継(名前と意味を一致させる)。is_staff は移行の間だけ残す

ALTER TABLE public.m_team
    ADD COLUMN IF NOT EXISTS visibility text NOT NULL DEFAULT 'public',
    ADD COLUMN IF NOT EXISTS settings_policy text NOT NULL DEFAULT 'members';

ALTER TABLE public.accounts_user
    ADD COLUMN IF NOT EXISTS is_guest boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS is_system_admin boolean NOT NULL DEFAULT false;

UPDATE public.accounts_user
SET is_system_admin = is_staff
WHERE is_system_admin IS DISTINCT FROM is_staff;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'm_team_visibility_check') THEN
        ALTER TABLE public.m_team
            ADD CONSTRAINT m_team_visibility_check CHECK (visibility IN ('public', 'private'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'm_team_settings_policy_check') THEN
        ALTER TABLE public.m_team
            ADD CONSTRAINT m_team_settings_policy_check CHECK (settings_policy IN ('members', 'owners'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'accounts_user_admin_not_guest') THEN
        ALTER TABLE public.accounts_user
            ADD CONSTRAINT accounts_user_admin_not_guest CHECK (NOT (is_system_admin AND is_guest));
    END IF;
END $$;

COMMENT ON COLUMN public.accounts_user.is_staff IS
    'DEPRECATED: is_system_admin を使う(アクセス制御の再設計。削除はフェーズ H の本番反映 + 7 日)';

-- 移行の間(フェーズ H まで)は、is_staff の変更を is_system_admin に写す(片方向)。
-- 管理者の付与・解除は、今は DB の is_staff で行われているため、新しい列と食い違わないようにする。
-- is_staff の列を削除するマイグレーション(フェーズ H + 7 日)で、このトリガーも削除する。
CREATE OR REPLACE FUNCTION public.access_copy_is_staff() RETURNS trigger AS $$
BEGIN
    IF NEW.is_staff IS DISTINCT FROM OLD.is_staff THEN
        NEW.is_system_admin := NEW.is_staff;
    END IF;
    RETURN NEW;
END $$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_user_copy_is_staff ON public.accounts_user;
CREATE TRIGGER accounts_user_copy_is_staff BEFORE UPDATE OF is_staff ON public.accounts_user
    FOR EACH ROW EXECUTE FUNCTION public.access_copy_is_staff();
