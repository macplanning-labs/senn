-- プロジェクトオーナー（作成者）を記録するカラムを追加。
-- 削除権限チェックに使用する。既存プロジェクトは最古のメンバーシップから補完する。

ALTER TABLE public.tickets_project
    ADD COLUMN IF NOT EXISTS owner_id bigint;

DO $$ BEGIN
    ALTER TABLE public.tickets_project
        ADD CONSTRAINT tickets_project_owner_id_fk_accounts_user_id
        FOREIGN KEY (owner_id) REFERENCES public.accounts_user(id)
        DEFERRABLE INITIALLY DEFERRED;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS tickets_project_owner_id_idx
    ON public.tickets_project USING btree (owner_id);

UPDATE public.tickets_project p
SET owner_id = sub.user_id
FROM (
    SELECT DISTINCT ON (project_id) project_id, user_id
    FROM public.tickets_project_membership
    ORDER BY project_id, created_at ASC, id ASC
) sub
WHERE p.id = sub.project_id
  AND p.owner_id IS NULL;
