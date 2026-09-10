-- Method P1: Cycle 自動完了・次 Cycle 自動作成（既定 ON）
ALTER TABLE tickets_project
  ADD COLUMN IF NOT EXISTS cycle_auto_complete boolean NOT NULL DEFAULT true;

ALTER TABLE tickets_project
  ADD COLUMN IF NOT EXISTS cycle_auto_create_next boolean NOT NULL DEFAULT true;

COMMENT ON COLUMN tickets_project.cycle_auto_complete IS
  'end_date 超過の active Cycle をスケジューラで自動完了する';
COMMENT ON COLUMN tickets_project.cycle_auto_create_next IS
  '持ち越し先 planned が無いとき次 Cycle を自動作成する';
