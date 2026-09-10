-- Fix production incident: 20260908110001 created provisional 1:1 teams
-- (m_team.slug LIKE 'prov-<project_id>-%') and backfilled tickets_ticket/t_cycle.team_id,
-- but never backfilled tickets_project.owner_team_id itself.
-- ai_agent_api::create_ticket resolves the team via project.owner_team, so every project
-- without owner_team_id fails legacy `project_prefix` + title ticket creation (500).

-- 1. FTFC (id 16) already has a hand-created team "Wipアプリ開発チーム" (id 7, slug wip-app-dev)
-- whose description explicitly names it as the FTFC owner. Use that instead of its
-- auto-generated provisional team (prov-16-ftf, left as an unused leftover for later cleanup).
UPDATE tickets_project
SET owner_team_id = 7
WHERE id = 16 AND owner_team_id IS NULL;

-- 2. All other projects without owner_team_id: link to their own provisional team.
UPDATE tickets_project tp
SET owner_team_id = mt.id
FROM m_team mt
WHERE mt.slug LIKE 'prov-' || tp.id || '-%'
AND tp.owner_team_id IS NULL;
