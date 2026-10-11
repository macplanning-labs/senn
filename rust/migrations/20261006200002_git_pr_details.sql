-- GitHub 連携 Phase 2: PR の詳細(Draft・レビュー結果・CI・変更量)。追加のみ。
-- 設計: docs/design/詳細設計_GitHub連携Phase2.md §5

ALTER TABLE t_git_event
    ADD COLUMN IF NOT EXISTS is_draft      boolean      NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS review_state  varchar(20)  NOT NULL DEFAULT 'none',   -- none|approved|changes_requested
    ADD COLUMN IF NOT EXISTS ci_state      varchar(20)  NOT NULL DEFAULT 'none',   -- none|pending|success|failure
    ADD COLUMN IF NOT EXISTS reviewers     text[]       NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS additions     integer,
    ADD COLUMN IF NOT EXISTS deletions     integer,
    ADD COLUMN IF NOT EXISTS changed_files integer,
    ADD COLUMN IF NOT EXISTS base_branch   varchar(200) NOT NULL DEFAULT '',
    -- このチケットがどう書かれていたか: implicit | closing | reference。reference は紐付けのみで、遷移しない
    ADD COLUMN IF NOT EXISTS key_intent    varchar(10)  NOT NULL DEFAULT 'implicit';

-- PR ごと・チェックごとの最新の状態(CI の集計用)
CREATE TABLE IF NOT EXISTS t_git_check_run (
    integration_id bigint       NOT NULL,
    pr_number      integer      NOT NULL,
    name           varchar(200) NOT NULL,
    status         varchar(20)  NOT NULL,        -- queued | in_progress | completed
    conclusion     varchar(20),                  -- success | failure | …(completed のとき)
    updated_at     timestamptz  NOT NULL DEFAULT now(),
    PRIMARY KEY (integration_id, pr_number, name)
);
