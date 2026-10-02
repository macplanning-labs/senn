-- アクセス制御の再設計 フェーズ E-3(詳細設計書 §3.5)
-- 役割・有効性・公開区分の変更で、リアルタイムの購読を計算し直す(今は所属の変更だけが対象)。
-- 受け手は dispatcher.rs(senn_access): 数値 = そのユーザーの接続、'all' = 接続中の全員。
-- 再実行しても失敗しない形にする(CREATE OR REPLACE / DROP TRIGGER IF EXISTS)。

CREATE OR REPLACE FUNCTION public.sync_notify_user_access() RETURNS trigger AS $$
BEGIN
  IF NEW.is_active IS DISTINCT FROM OLD.is_active
     OR NEW.is_guest IS DISTINCT FROM OLD.is_guest
     OR NEW.is_system_admin IS DISTINCT FROM OLD.is_system_admin THEN
    PERFORM pg_notify('senn_access', NEW.id::text);
  END IF;
  RETURN NEW;
END $$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_user_access_notify ON public.accounts_user;
CREATE TRIGGER accounts_user_access_notify AFTER UPDATE ON public.accounts_user
  FOR EACH ROW EXECUTE FUNCTION public.sync_notify_user_access();

-- 公開区分の変更は、全接続の再計算を求める(チームは約 30。変更はまれ)
CREATE OR REPLACE FUNCTION public.sync_notify_team_visibility() RETURNS trigger AS $$
BEGIN
  IF NEW.visibility IS DISTINCT FROM OLD.visibility THEN
    PERFORM pg_notify('senn_access', 'all');
  END IF;
  RETURN NEW;
END $$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS m_team_visibility_notify ON public.m_team;
CREATE TRIGGER m_team_visibility_notify AFTER UPDATE ON public.m_team
  FOR EACH ROW EXECUTE FUNCTION public.sync_notify_team_visibility();
