-- GitHub 連携 Phase 2a: トリガーごとのステータス設定。追加のみ。
-- 設計: docs/design/詳細設計_GitHub連携Phase2.md §3
--
-- 行が無いトリガーは「標準」(現在の動作)。画面で「標準に戻す」と、その行を消す。
CREATE TABLE IF NOT EXISTS t_git_status_rule (
    id              bigserial PRIMARY KEY,
    project_id      bigint REFERENCES tickets_project(id) ON DELETE CASCADE,
    team_id         bigint REFERENCES m_team(id) ON DELETE CASCADE,
    trigger         varchar(30) NOT NULL,
    action          varchar(10) NOT NULL,          -- 'set' | 'none'
    target_slug     varchar(50),                   -- action='set' のとき、どちらか一方
    target_category varchar(20),
    updated_by_id   bigint,
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT git_status_rule_scope CHECK ((project_id IS NULL) <> (team_id IS NULL)),
    CONSTRAINT git_status_rule_action CHECK (action IN ('set', 'none')),
    CONSTRAINT git_status_rule_target CHECK (
        (action = 'none' AND target_slug IS NULL AND target_category IS NULL)
        OR (action = 'set' AND ((target_slug IS NOT NULL) <> (target_category IS NOT NULL)))
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_git_status_rule_project
    ON t_git_status_rule (project_id, trigger) WHERE project_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_git_status_rule_team
    ON t_git_status_rule (team_id, trigger) WHERE team_id IS NOT NULL;
