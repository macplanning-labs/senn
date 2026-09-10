-- 20260907235959で先行追加したt_cycle.team_idは、20260908110001が
-- 「ALTER TABLE t_cycle ADD COLUMN team_id ...」を実行する際に列の重複エラーに
-- ならないよう、20260908110001が実行される前に一旦戻す。
--
-- ただし20260908110001が既に適用済みの環境(staging等)では、その列には既に
-- バックフィル済みの実データが入っているため、ここで消してはならない。
-- 20260908110001が未適用(_sqlx_migrationsに記録が無い)の場合のみ列を戻す。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM _sqlx_migrations WHERE version = 20260908110001
    ) THEN
        ALTER TABLE t_cycle DROP COLUMN IF EXISTS team_id;
    END IF;
END $$;
