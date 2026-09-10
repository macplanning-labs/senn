-- Add team_id column to wiki_page for Team Wiki support
-- Enforce that (project_id, slug) and (team_id, slug) are each unique when scoped
-- A page has either project_id or team_id (or both NULL for legacy workspace pages)

ALTER TABLE public.wiki_page
ADD COLUMN IF NOT EXISTS team_id bigint REFERENCES public.m_team(id) ON DELETE CASCADE;

-- Replace table UNIQUE(project_id, slug) with partial indexes so team-scoped rows
-- (project_id NULL) do not collide on slug across teams.
ALTER TABLE public.wiki_page
DROP CONSTRAINT IF EXISTS wiki_page_project_id_slug_345dc24a_uniq;

CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_page_project_slug ON public.wiki_page (project_id, slug)
WHERE project_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_wiki_page_team_slug ON public.wiki_page (team_id, slug)
WHERE team_id IS NOT NULL;

ALTER TABLE public.wiki_page
DROP CONSTRAINT IF EXISTS check_wiki_page_scope;

ALTER TABLE public.wiki_page
ADD CONSTRAINT check_wiki_page_scope
CHECK (
  (project_id IS NOT NULL AND team_id IS NULL)
  OR (project_id IS NULL AND team_id IS NOT NULL)
  OR (project_id IS NULL AND team_id IS NULL)
);