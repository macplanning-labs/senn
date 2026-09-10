-- Add nullable team_id to tickets_ticket and t_cycle
-- Phase 1 of Team-first model migration

-- 1. Add team_id column to tickets_ticket (nullable FK to m_team)
ALTER TABLE tickets_ticket
ADD COLUMN team_id BIGINT REFERENCES m_team(id) ON DELETE SET NULL;

-- 2. Add team_id column to t_cycle (nullable FK to m_team)
ALTER TABLE t_cycle
ADD COLUMN team_id BIGINT REFERENCES m_team(id) ON DELETE SET NULL;

-- 3. Backfill: Copy owner_team_id from project to tickets_ticket and t_cycle
UPDATE tickets_ticket tt
SET team_id = tp.owner_team_id
FROM tickets_project tp
WHERE tt.project_id = tp.id AND tp.owner_team_id IS NOT NULL;

UPDATE t_cycle tc
SET team_id = tp.owner_team_id
FROM tickets_project tp
WHERE tc.project_id = tp.id AND tp.owner_team_id IS NOT NULL;

-- 4. For projects without owner_team_id, create a provisional 1:1 team
-- Generate team for each project without owner_team_id
INSERT INTO m_team (name, slug, description, icon, color, slack_webhook_url, is_active, created_at)
SELECT
    tp.name || ' チーム' AS name,
    'prov-' || tp.id || '-' || LOWER(REPLACE(SUBSTR(tp.prefix, 1, 3), '-', '_')) AS slug,
    '暫定作成されたチーム（後で手動整理可）' AS description,
    '👥' AS icon,
    '#808080' AS color,
    '' AS slack_webhook_url,
    true AS is_active,
    NOW() AS created_at
FROM tickets_project tp
WHERE tp.owner_team_id IS NULL
AND NOT EXISTS (
    SELECT 1 FROM m_team mt
    WHERE mt.slug LIKE 'prov-' || tp.id || '-%'
);

-- 5. Backfill created provisional teams to tickets_ticket
UPDATE tickets_ticket tt
SET team_id = mt.id
FROM tickets_project tp
JOIN m_team mt ON mt.slug LIKE 'prov-' || tp.id || '-%'
WHERE tt.project_id = tp.id AND tp.owner_team_id IS NULL AND tt.team_id IS NULL;

-- 6. Backfill created provisional teams to t_cycle
UPDATE t_cycle tc
SET team_id = mt.id
FROM tickets_project tp
JOIN m_team mt ON mt.slug LIKE 'prov-' || tp.id || '-%'
WHERE tc.project_id = tp.id AND tp.owner_team_id IS NULL AND tc.team_id IS NULL;
