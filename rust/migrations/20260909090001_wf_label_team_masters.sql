-- Wave 4 H3: WF/Label を Team マスタでも使えるようにする。既存 Project 行は残す。

ALTER TABLE t_workflow_status
    ALTER COLUMN project_id DROP NOT NULL;

ALTER TABLE m_label
    ALTER COLUMN project_id DROP NOT NULL;

ALTER TABLE t_workflow_status
    DROP CONSTRAINT IF EXISTS t_workflow_status_project_id_slug_3c27f479_uniq;

ALTER TABLE m_label
    DROP CONSTRAINT IF EXISTS unique_label_per_project;

CREATE UNIQUE INDEX IF NOT EXISTS unique_wf_slug_per_project
    ON t_workflow_status (project_id, slug)
    WHERE project_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS unique_wf_slug_per_team
    ON t_workflow_status (team_id, slug)
    WHERE team_id IS NOT NULL AND project_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS unique_wf_slug_workspace
    ON t_workflow_status (slug)
    WHERE project_id IS NULL AND team_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS unique_label_name_per_project
    ON m_label (project_id, name)
    WHERE project_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS unique_label_name_per_team
    ON m_label (team_id, name)
    WHERE team_id IS NOT NULL AND project_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS unique_label_name_workspace
    ON m_label (name)
    WHERE project_id IS NULL AND team_id IS NULL;

-- H5: Saved View を Team にも紐づけられる
ALTER TABLE t_saved_view
    DROP CONSTRAINT IF EXISTS t_saved_view_project_id_owner_id_name_key;

ALTER TABLE t_saved_view
    ALTER COLUMN project_id DROP NOT NULL;

ALTER TABLE t_saved_view
    ADD COLUMN IF NOT EXISTS team_id BIGINT REFERENCES m_team(id) ON DELETE CASCADE;

CREATE UNIQUE INDEX IF NOT EXISTS unique_saved_view_per_project_owner_name
    ON t_saved_view (project_id, owner_id, name)
    WHERE project_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS unique_saved_view_per_team_owner_name
    ON t_saved_view (team_id, owner_id, name)
    WHERE team_id IS NOT NULL;
