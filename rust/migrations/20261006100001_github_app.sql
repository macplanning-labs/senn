-- GitHub App 連携(かんたん接続)。追加のみ。既存テーブルは列追加(NULL 許容)だけ。
-- 設計: docs/design/詳細設計_GitHub連携かんたん接続.md

-- App は環境に 1 つ(行は 1 つだけ)
CREATE TABLE IF NOT EXISTS t_github_app (
    id                  smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    app_id              bigint NOT NULL,
    slug                varchar(100) NOT NULL,
    name                varchar(100) NOT NULL,
    html_url            varchar(500) NOT NULL,
    owner_login         varchar(100) NOT NULL DEFAULT '',
    client_id           varchar(100) NOT NULL,
    client_secret_enc   text NOT NULL,
    webhook_secret_enc  text NOT NULL,
    private_key_enc     text NOT NULL,
    created_by_id       bigint,
    created_at          timestamptz NOT NULL DEFAULT now()
);

-- インストール(組織/ユーザーごと)
CREATE TABLE IF NOT EXISTS t_github_installation (
    installation_id      bigint PRIMARY KEY,
    account_login        varchar(100) NOT NULL,
    account_type         varchar(20)  NOT NULL,
    repository_selection varchar(20)  NOT NULL,
    suspended_at         timestamptz,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now()
);

-- Manifest / インストールの CSRF 対策(使い捨て)
CREATE TABLE IF NOT EXISTS t_github_setup_state (
    state        varchar(64) PRIMARY KEY,
    purpose      varchar(20) NOT NULL,
    user_id      bigint NOT NULL,
    return_path  varchar(300) NOT NULL DEFAULT '/',
    expires_at   timestamptz NOT NULL
);

-- 既存の連携に「App 経由」と診断情報を足す
ALTER TABLE t_git_integration
    ADD COLUMN IF NOT EXISTS installation_id      bigint,
    ADD COLUMN IF NOT EXISTS last_delivery_at     timestamptz,
    ADD COLUMN IF NOT EXISTS last_delivery_event  varchar(40),
    ADD COLUMN IF NOT EXISTS last_delivery_result varchar(20),
    ADD COLUMN IF NOT EXISTS last_delivery_note   varchar(300);

-- 1 リポジトリ = App 連携は 1 つ(同じ push を二重に処理しない)
CREATE UNIQUE INDEX IF NOT EXISTS idx_git_integration_app_repo
    ON t_git_integration (repository_url) WHERE installation_id IS NOT NULL;
