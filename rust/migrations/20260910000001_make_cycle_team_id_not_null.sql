-- Wave 7 K2: Make t_cycle.team_id NOT NULL
-- Backfill remaining nulls, verify completion, then add constraint

-- 1. Backfill: Copy owner_team_id from project where team_id is still null
UPDATE t_cycle
SET team_id = (SELECT owner_team_id FROM tickets_project WHERE id = t_cycle.project_id)
WHERE team_id IS NULL AND project_id IS NOT NULL;

-- 2. Verification comment: Run manually after migration to confirm no nulls remain
-- SELECT COUNT(*) as null_team_id_count FROM t_cycle WHERE team_id IS NULL;
-- Result should be: 0 (all cycles now have team_id assigned)

-- 3. Add NOT NULL constraint to team_id column
ALTER TABLE t_cycle ALTER COLUMN team_id SET NOT NULL;
