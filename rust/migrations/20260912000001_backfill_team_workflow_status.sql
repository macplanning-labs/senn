-- 既存チームへの、チームレベル(project_id無し)ワークフローステータスの遡及投入。
--
-- 背景: team_repo.rs::create_team が project_id NULL のチケット作成時に必ず参照される
-- team_id スコープの t_workflow_status を作らないまま運用されていたため、project_id
-- を指定せずチーム画面から直接チケットを作成すると「このチームに存在しないステータス
-- です」で必ず失敗していた(reject_unknown_team_status, tickets_api.rs)。
-- create_team 側は別途修正済み(新規チームは作成時に自動投入)だが、既存チームには
-- 遡及して同じ既定セットを投入する必要がある。
--
-- 対象: t_workflow_status に team_id スコープ(team_id = 該当チーム AND project_id IS NULL)
-- の行を1件も持たないチーム全て。既にteam_idスコープの行を持つチームはスキップする
-- (ON CONFLICT に加えてWHERE NOT EXISTSで二重に防御し、誤って重複投入しないようにする)。

INSERT INTO t_workflow_status (slug, name, category, color, position, is_default, team_id, project_id)
SELECT d.slug, d.name, d.category, d.color, d.position, d.is_default, t.id, NULL
FROM m_team t
CROSS JOIN (
    VALUES
        ('backlog', 'Backlog', 'backlog', '#666666', 0, false),
        ('open', 'Todo', 'unstarted', '#a0a0a0', 1, true),
        ('in_progress', 'In Progress', 'started', '#f5a623', 2, false),
        ('resolved', 'Done', 'completed', '#50e3c2', 3, false),
        ('closed', 'Closed', 'completed', '#5c6cff', 4, false),
        ('canceled', 'Cancelled', 'cancelled', '#ff4d4f', 5, false)
) AS d(slug, name, category, color, position, is_default)
WHERE NOT EXISTS (
    SELECT 1 FROM t_workflow_status ws
    WHERE ws.team_id = t.id AND ws.project_id IS NULL
)
ON CONFLICT (team_id, slug) WHERE team_id IS NOT NULL AND project_id IS NULL DO NOTHING;
