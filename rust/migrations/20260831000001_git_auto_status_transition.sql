ALTER TABLE t_git_integration
  ADD COLUMN IF NOT EXISTS auto_status_transition boolean NOT NULL DEFAULT true;

COMMENT ON COLUMN t_git_integration.auto_status_transition IS
  'キー付き push/PR merge でチケット status を自動更新する';
