-- GitHub 連携 Phase 2: 受信履歴(連携ごとに最新 50 件)。追加のみ。
-- 設計: docs/design/詳細設計_GitHub連携Phase2.md §7
CREATE TABLE IF NOT EXISTS t_git_delivery (
    id             bigserial    PRIMARY KEY,
    integration_id bigint       NOT NULL,
    event          varchar(40)  NOT NULL,
    result         varchar(20)  NOT NULL,        -- ok | no_ticket | ignored
    note           varchar(300) NOT NULL DEFAULT '',
    received_at    timestamptz  NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_git_delivery_integration
    ON t_git_delivery (integration_id, received_at DESC, id DESC);
