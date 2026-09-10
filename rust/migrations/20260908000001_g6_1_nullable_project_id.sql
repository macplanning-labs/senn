-- G6-1: tickets/cycles の project_id を NULL 可にする
-- m_team.prefix を追加し、既存 Team には代表 Project prefix（無ければ slug）を埋める
-- WF/Label に nullable team_id（U7 下地。既存 project_id は残す）

-- 1. Add prefix column to m_team
ALTER TABLE m_team
ADD COLUMN prefix VARCHAR(20);

-- 空の Team には、owner になっている Project のうち id が最小の prefix を入れる。
-- それが無ければ slug の英数字を大文字にしたものを使う。Team 行は消さない。
UPDATE m_team t
SET prefix = sub.prefix
FROM (
    SELECT DISTINCT ON (p.owner_team_id)
        p.owner_team_id,
        p.prefix
    FROM tickets_project p
    WHERE p.owner_team_id IS NOT NULL
      AND p.prefix IS NOT NULL
      AND length(btrim(p.prefix)) > 0
    ORDER BY p.owner_team_id, p.id
) sub
WHERE t.id = sub.owner_team_id
  AND t.prefix IS NULL;

UPDATE m_team
SET prefix = upper(left(regexp_replace(slug, '[^a-zA-Z0-9]', '', 'g'), 20))
WHERE prefix IS NULL OR btrim(prefix) = '';

-- 2. Make tickets_ticket.project_id nullable
ALTER TABLE tickets_ticket
ALTER COLUMN project_id DROP NOT NULL;

-- 3. Make t_cycle.project_id nullable
ALTER TABLE t_cycle
ALTER COLUMN project_id DROP NOT NULL;

-- 4. 残っている Cycle.team_id を owner_team から埋める（Wave 2 でほぼ済み）
UPDATE t_cycle c
SET team_id = p.owner_team_id
FROM tickets_project p
WHERE c.project_id = p.id
  AND c.team_id IS NULL
  AND p.owner_team_id IS NOT NULL;

-- 5. UNIQUE (project_id, number) は NULL project で一意にならない。
-- 既存データは同一 Team でも Project ごとに number=1 があり得るので、
-- 全行 UNIQUE (team_id, number) にはしない。
ALTER TABLE t_cycle
DROP CONSTRAINT IF EXISTS unique_cycle_number_per_project;

CREATE UNIQUE INDEX IF NOT EXISTS unique_cycle_number_per_project
    ON t_cycle (project_id, number)
    WHERE project_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS unique_cycle_number_per_team_no_project
    ON t_cycle (team_id, number)
    WHERE project_id IS NULL AND team_id IS NOT NULL;

-- 6. Add nullable team_id to t_workflow_status
ALTER TABLE t_workflow_status
ADD COLUMN team_id BIGINT REFERENCES m_team(id) ON DELETE SET NULL;

-- 7. Add nullable team_id to m_label
ALTER TABLE m_label
ADD COLUMN team_id BIGINT REFERENCES m_team(id) ON DELETE SET NULL;
