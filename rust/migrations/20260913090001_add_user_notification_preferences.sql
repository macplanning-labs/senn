-- ユーザーごとのイベント種別別メール通知ON/OFF設定。
-- 行が存在しないカテゴリはアプリケーション側でデフォルトON(email_enabled=true)として扱う。
CREATE TABLE IF NOT EXISTS accounts_user_notification_preference (
    id bigserial PRIMARY KEY,
    user_id integer NOT NULL REFERENCES accounts_user(id) ON DELETE CASCADE,
    category varchar(32) NOT NULL,
    email_enabled boolean NOT NULL DEFAULT true,
    UNIQUE (user_id, category)
);

COMMENT ON TABLE accounts_user_notification_preference IS 'ユーザーごとのイベント種別(NotificationCategory)別メール通知ON/OFF設定。行が無いカテゴリはアプリ側でデフォルトON扱い。';
COMMENT ON COLUMN accounts_user_notification_preference.category IS 'NotificationCategory::as_db_str()の値(assigned/commented/status_changed/updated/mentioned/due_soon/overdue)のいずれか';
