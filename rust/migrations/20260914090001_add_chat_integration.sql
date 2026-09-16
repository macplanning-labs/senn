-- project または team にスコープしたチャット通知連携(Slack/Google Chat/Chatwork/Teams)。
-- t_git_integration と同じ「project/team排他スコープ」の考え方に倣う。
CREATE TABLE IF NOT EXISTS t_chat_integration (
    id bigserial PRIMARY KEY,
    project_id integer NULL REFERENCES tickets_project(id) ON DELETE CASCADE,
    team_id integer NULL REFERENCES m_team(id) ON DELETE CASCADE,
    provider varchar(16) NOT NULL,
    webhook_url text NULL,
    api_token text NULL,
    room_id varchar(64) NULL,
    enabled_categories text[] NOT NULL DEFAULT '{assigned}',
    is_active boolean NOT NULL DEFAULT true,
    created_by_id integer NULL REFERENCES accounts_user(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT NOW(),
    CONSTRAINT chat_integration_scope_check CHECK (
        (project_id IS NOT NULL AND team_id IS NULL) OR (project_id IS NULL AND team_id IS NOT NULL)
    ),
    CONSTRAINT chat_integration_provider_check CHECK (
        provider IN ('slack', 'google_chat', 'teams', 'chatwork')
    )
);

CREATE INDEX IF NOT EXISTS idx_chat_integration_project ON t_chat_integration(project_id) WHERE project_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_chat_integration_team ON t_chat_integration(team_id) WHERE team_id IS NOT NULL;

COMMENT ON TABLE t_chat_integration IS 'project/teamスコープのチャット通知連携。provider=slack/google_chat/teamsはwebhook_url、provider=chatworkはapi_token+room_idを使う。';
COMMENT ON COLUMN t_chat_integration.enabled_categories IS 'NotificationCategory::as_db_str()の値の配列。このイベントが発生した時だけこの連携へ送信する。';
