ALTER TABLE notifications_notification
  ADD COLUMN IF NOT EXISTS is_hidden boolean NOT NULL DEFAULT false;

CREATE INDEX IF NOT EXISTS idx_notif_user_visible
  ON notifications_notification (user_id, is_hidden, is_read, created_at DESC);
