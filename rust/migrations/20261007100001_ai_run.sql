-- AI実行履歴テーブル。1実行1行。着手/完了/失敗を記録し、ステータス遷移とコメント/通知を制御する。
-- 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.1 / §3.5(SENN チケット DEMO-000180)
-- result が NULL の行は「未完了」。problem_notified_at は、エラー/未報告をコメント+通知した時刻(1実行につき1回)。
CREATE TABLE IF NOT EXISTS t_ai_run (
    id                      bigserial     PRIMARY KEY,
    run_uuid                uuid          NOT NULL UNIQUE,
    ticket_id               bigint        NOT NULL REFERENCES public.tickets_ticket(id) ON DELETE CASCADE,
    trigger_type            varchar(20)   NOT NULL DEFAULT 'manual_copy' CHECK (trigger_type IN ('manual_copy','webhook','api')),
    tool                    varchar(30)   NOT NULL DEFAULT 'claude_code',
    model                   varchar(80)   NOT NULL DEFAULT '',
    triggered_by_id         bigint        NULL REFERENCES public.accounts_user(id) ON DELETE SET NULL,
    prompt_generated_at     timestamptz   NOT NULL DEFAULT now(),
    started_at              timestamptz   NULL,
    ended_at                timestamptz   NULL,
    result                  varchar(20)   NULL CHECK (result IN ('resolved','blocked','aborted','error')),
    error_detail            text          NOT NULL DEFAULT '',
    summary                 text          NOT NULL DEFAULT '',
    problem_notified_at     timestamptz   NULL,
    input_tokens            bigint        NULL,
    output_tokens           bigint        NULL,
    cache_read_tokens       bigint        NULL,
    cache_creation_tokens   bigint        NULL,
    usd_equivalent          double precision NULL,
    created_at              timestamptz   NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ai_run_ticket_created
    ON t_ai_run (ticket_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_run_unstarted_overdue
    ON t_ai_run (prompt_generated_at)
    WHERE result IS NULL AND problem_notified_at IS NULL AND started_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_ai_run_unfinished_overdue
    ON t_ai_run (started_at)
    WHERE result IS NULL AND problem_notified_at IS NULL AND started_at IS NOT NULL;
