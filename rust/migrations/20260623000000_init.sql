-- ============================================================
-- WIP（プロジェクト管理ツール）初期テーブル作成
-- ============================================================

-- ============================================================
-- ユーザー
-- ============================================================
CREATE TABLE m_users (
    id SERIAL PRIMARY KEY,
    username VARCHAR(150) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name VARCHAR(100) NOT NULL DEFAULT '',
    email VARCHAR(254) NOT NULL DEFAULT '',
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_staff BOOLEAN NOT NULL DEFAULT false,
    must_change_password BOOLEAN NOT NULL DEFAULT false,
    email_notifications_enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- TOTP デバイス
-- ============================================================
CREATE TABLE s_totp_devices (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL UNIQUE REFERENCES m_users(id) ON DELETE CASCADE,
    secret VARCHAR(64) NOT NULL,
    confirmed BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- WebAuthn パスキー
-- ============================================================
CREATE TABLE s_webauthn_credentials (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES m_users(id) ON DELETE CASCADE,
    credential_id BYTEA NOT NULL UNIQUE,
    public_key BYTEA NOT NULL,
    sign_count INTEGER NOT NULL DEFAULT 0,
    name VARCHAR(100) NOT NULL DEFAULT 'マイデバイス',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- セッション
-- ============================================================
CREATE TABLE s_sessions (
    id TEXT PRIMARY KEY,
    data BYTEA NOT NULL,
    expiry_date TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_sessions_expiry ON s_sessions(expiry_date);

-- ============================================================
-- プロジェクト
-- ============================================================
CREATE TABLE m_projects (
    id SERIAL PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    prefix VARCHAR(20) NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- カテゴリー（2階層: フェーズ + カテゴリー）
-- ============================================================
CREATE TABLE m_categories (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(50) NOT NULL UNIQUE,
    level SMALLINT NOT NULL DEFAULT 1 CHECK (level IN (1, 2)),
    parent_id INTEGER REFERENCES m_categories(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    color VARCHAR(7) NOT NULL DEFAULT '#6366f1',
    CONSTRAINT category_level_parent CHECK (
        (level = 1 AND parent_id IS NULL) OR
        (level = 2 AND parent_id IS NOT NULL)
    )
);

-- ============================================================
-- マイルストーン
-- ============================================================
CREATE TABLE m_milestones (
    id SERIAL PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    due_date DATE,
    description TEXT NOT NULL DEFAULT '',
    project_id INTEGER REFERENCES m_projects(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- チケット（親子階層）
-- ============================================================
CREATE TABLE t_tickets (
    id SERIAL PRIMARY KEY,
    ticket_key VARCHAR(20) NOT NULL UNIQUE,
    title VARCHAR(500) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status VARCHAR(20) NOT NULL DEFAULT '未対応',
    priority VARCHAR(10) NOT NULL DEFAULT '中',
    ticket_type VARCHAR(20) NOT NULL DEFAULT '課題',
    parent_id INTEGER REFERENCES t_tickets(id) ON DELETE CASCADE,
    category_id INTEGER REFERENCES m_categories(id) ON DELETE SET NULL,
    project_id INTEGER REFERENCES m_projects(id) ON DELETE SET NULL,
    author_id INTEGER NOT NULL REFERENCES m_users(id) ON DELETE RESTRICT,
    assignee_id INTEGER REFERENCES m_users(id) ON DELETE SET NULL,
    milestone_id INTEGER REFERENCES m_milestones(id) ON DELETE SET NULL,
    start_date DATE,
    due_date DATE,
    gantt_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at TIMESTAMPTZ
);
CREATE INDEX idx_ticket_key ON t_tickets(ticket_key);
CREATE INDEX idx_ticket_status_due ON t_tickets(status, due_date);
CREATE INDEX idx_ticket_assignee_status ON t_tickets(assignee_id, status);
CREATE INDEX idx_ticket_category_status ON t_tickets(category_id, status);
CREATE INDEX idx_ticket_parent_status ON t_tickets(parent_id, status);
CREATE INDEX idx_ticket_milestone ON t_tickets(milestone_id, status);
CREATE INDEX idx_ticket_gantt ON t_tickets(gantt_order);

-- ============================================================
-- ウォッチ（多対多）
-- ============================================================
CREATE TABLE t_ticket_watchers (
    ticket_id INTEGER NOT NULL REFERENCES t_tickets(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES m_users(id) ON DELETE CASCADE,
    PRIMARY KEY (ticket_id, user_id)
);

-- ============================================================
-- コメント
-- ============================================================
CREATE TABLE t_comments (
    id SERIAL PRIMARY KEY,
    ticket_id INTEGER NOT NULL REFERENCES t_tickets(id) ON DELETE CASCADE,
    author_id INTEGER NOT NULL REFERENCES m_users(id) ON DELETE RESTRICT,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- 添付ファイル
-- ============================================================
CREATE TABLE t_attachments (
    id SERIAL PRIMARY KEY,
    ticket_id INTEGER NOT NULL REFERENCES t_tickets(id) ON DELETE CASCADE,
    comment_id INTEGER REFERENCES t_comments(id) ON DELETE CASCADE,
    uploader_id INTEGER NOT NULL REFERENCES m_users(id) ON DELETE RESTRICT,
    filename VARCHAR(255) NOT NULL,
    file_path TEXT NOT NULL,
    file_size INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- ステータス変更履歴（バーンダウンチャート用）
-- ============================================================
CREATE TABLE h_ticket_status (
    id SERIAL PRIMARY KEY,
    ticket_id INTEGER NOT NULL REFERENCES t_tickets(id) ON DELETE CASCADE,
    old_status VARCHAR(20) NOT NULL DEFAULT '',
    new_status VARCHAR(20) NOT NULL,
    changed_by_id INTEGER REFERENCES m_users(id) ON DELETE SET NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_history_ticket_date ON h_ticket_status(ticket_id, changed_at);

-- ============================================================
-- 通知（In-app）
-- ============================================================
CREATE TABLE t_notifications (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES m_users(id) ON DELETE CASCADE,
    ticket_id INTEGER REFERENCES t_tickets(id) ON DELETE CASCADE,
    category VARCHAR(20) NOT NULL DEFAULT 'commented',
    title VARCHAR(200) NOT NULL,
    message TEXT NOT NULL DEFAULT '',
    is_read BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_notif_user_unread ON t_notifications(user_id, is_read, created_at DESC);

-- ============================================================
-- 通知送信ログ（メール重複防止）
-- ============================================================
CREATE TABLE h_notification_logs (
    id SERIAL PRIMARY KEY,
    ticket_id INTEGER NOT NULL REFERENCES t_tickets(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES m_users(id) ON DELETE CASCADE,
    notification_type VARCHAR(20) NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_notif_dedup ON h_notification_logs(ticket_id, user_id, notification_type, sent_at);

-- ============================================================
-- Wiki ページ
-- ============================================================
CREATE TABLE t_wiki_pages (
    id SERIAL PRIMARY KEY,
    project_id INTEGER REFERENCES m_projects(id) ON DELETE CASCADE,
    title VARCHAR(300) NOT NULL,
    slug VARCHAR(300) NOT NULL,
    category VARCHAR(20) NOT NULL DEFAULT 'other',
    content TEXT NOT NULL DEFAULT '',
    author_id INTEGER NOT NULL REFERENCES m_users(id) ON DELETE RESTRICT,
    last_editor_id INTEGER REFERENCES m_users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, slug)
);

-- ============================================================
-- Wiki 編集履歴
-- ============================================================
CREATE TABLE t_wiki_revisions (
    id SERIAL PRIMARY KEY,
    page_id INTEGER NOT NULL REFERENCES t_wiki_pages(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    editor_id INTEGER REFERENCES m_users(id) ON DELETE SET NULL,
    comment VARCHAR(300) NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- 休日（ガントカレンダー用）
-- ============================================================
CREATE TABLE m_holidays (
    id SERIAL PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    recurring BOOLEAN NOT NULL DEFAULT false
);
