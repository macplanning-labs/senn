-- Add is_shared column to t_saved_view for shared view feature
ALTER TABLE t_saved_view
  ADD COLUMN is_shared boolean NOT NULL DEFAULT false;

COMMENT ON COLUMN t_saved_view.is_shared IS 'true: 同一 project の全メンバーが読取可。false: 所有者のみ';
