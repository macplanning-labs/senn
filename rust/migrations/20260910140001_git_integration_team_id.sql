-- Team Git: t_git_integration.team_id を足し、Project 連携は残す
-- ちょうど一方のスコープ（project_id XOR team_id）を必須にする

ALTER TABLE public.t_git_integration
ADD COLUMN IF NOT EXISTS team_id bigint REFERENCES public.m_team(id) ON DELETE CASCADE;

-- Team-only 行を許すため project_id を NULL 可に
ALTER TABLE public.t_git_integration
ALTER COLUMN project_id DROP NOT NULL;

-- (project_id, repository_url) UNIQUE は project_id NULL で衝突しうるので partial に差し替え
ALTER TABLE public.t_git_integration
DROP CONSTRAINT IF EXISTS unique_project_repo;

CREATE UNIQUE INDEX IF NOT EXISTS idx_git_integration_project_repo
ON public.t_git_integration (project_id, repository_url)
WHERE project_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_git_integration_team_repo
ON public.t_git_integration (team_id, repository_url)
WHERE team_id IS NOT NULL;

ALTER TABLE public.t_git_integration
DROP CONSTRAINT IF EXISTS check_git_integration_scope;

ALTER TABLE public.t_git_integration
ADD CONSTRAINT check_git_integration_scope
CHECK (
  (project_id IS NOT NULL AND team_id IS NULL)
  OR (project_id IS NULL AND team_id IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_git_integration_team_id ON public.t_git_integration (team_id);
