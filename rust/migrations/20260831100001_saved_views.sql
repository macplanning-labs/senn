CREATE TABLE IF NOT EXISTS t_saved_view (
  id bigserial PRIMARY KEY,
  project_id bigint NOT NULL REFERENCES tickets_project(id) ON DELETE CASCADE,
  owner_id bigint NOT NULL REFERENCES accounts_user(id) ON DELETE CASCADE,
  name varchar(100) NOT NULL,
  filters jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (project_id, owner_id, name)
);

CREATE INDEX IF NOT EXISTS idx_saved_view_owner_project
  ON t_saved_view (project_id, owner_id);

COMMENT ON TABLE t_saved_view IS '個人用チケット一覧 Saved View（Method P3）';
COMMENT ON COLUMN t_saved_view.filters IS
  'JSON: search/status/priority/due/status_in（いずれも string、空は未指定）';
