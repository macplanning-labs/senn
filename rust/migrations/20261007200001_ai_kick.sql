-- 「AIに対応させる」ワンクリック(社内ランナー方式)の土台。
-- 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.6(SENN チケット DEMO-000180 Phase 5)
--
-- - t_ai_runner: ランナー(AI を実行する社内マシン)の認証。トークンは**ハッシュだけ**を保存する。
--   署名用の秘密(依頼の内容の改ざん検知。HMAC)は、暗号化して保存する。
-- - t_ai_run: キューの状態(依頼・取得・停止)と、暴走の防止に使う値(ハートビート・トークン量)を追加する。
-- - tickets_project: プロジェクトごとの「AI 自動実行」の許可と、ランナーが作業するリポジトリ。**標準は OFF**。
-- 追加のみ。適用済みの既存ファイルは変更しない。

CREATE TABLE IF NOT EXISTS t_ai_runner (
    id                  bigserial     PRIMARY KEY,
    name                varchar(80)   NOT NULL UNIQUE,
    token_hash          char(64)      NOT NULL UNIQUE,
    token_prefix        varchar(16)   NOT NULL,
    signing_secret_enc  text          NOT NULL,
    created_by_id       bigint        NULL REFERENCES public.accounts_user(id) ON DELETE SET NULL,
    created_at          timestamptz   NOT NULL DEFAULT now(),
    revoked_at          timestamptz   NULL,
    last_seen_at        timestamptz   NULL
);

ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS queued_at            timestamptz NULL;
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS claimed_at           timestamptz NULL;
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS claimed_by_runner_id bigint      NULL REFERENCES t_ai_runner(id) ON DELETE SET NULL;
-- ランナーに渡した指示の控え(監査用)と、そのハッシュ(起動者が確認した内容と同じかの確認に使う)
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS kick_prompt          text        NULL;
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS kick_prompt_hash     char(64)    NULL;
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS repo                 varchar(200) NULL;
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS base_branch          varchar(100) NULL;
-- この実行に課した上限(ターン数・分・加重トークン)。ランナー側の絶対上限と小さいほうが使われる
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS caps                 jsonb       NULL;
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS last_heartbeat_at    timestamptz NULL;
-- 加重トークン(入力+出力+キャッシュ作成+キャッシュ読み取りの 0.1 倍)。ハートビートで更新し、完了時に確定する
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS weighted_tokens      bigint      NOT NULL DEFAULT 0;
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS stop_requested_at    timestamptz NULL;
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS stop_reason          varchar(40) NULL;
-- 優先度の区分(emergency / high / normal / low)と、取得待ちの並び順(小さいほど先)。
-- 区分は、チケットの優先度と「他の未完了のチケットをブロックしているか」から、サーバーが決める(画面からは指定できない)
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS priority_class       varchar(12) NOT NULL DEFAULT 'normal';
ALTER TABLE t_ai_run ADD COLUMN IF NOT EXISTS priority_rank        smallint    NOT NULL DEFAULT 2;

-- 取得待ちのキュー(ランナーが古い順に取る)
CREATE INDEX IF NOT EXISTS idx_ai_run_queue
    ON t_ai_run (priority_rank, queued_at)
    WHERE queued_at IS NOT NULL AND claimed_at IS NULL AND result IS NULL AND stop_requested_at IS NULL;
-- 回数・予算の集計(ユーザー・チケット・期間)
CREATE INDEX IF NOT EXISTS idx_ai_run_kick_user ON t_ai_run (triggered_by_id, queued_at) WHERE queued_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_run_kick_ticket ON t_ai_run (ticket_id, queued_at) WHERE queued_at IS NOT NULL;

-- プロジェクトごとの「AI 自動実行」。標準は OFF。リポジトリは「owner/name」。
ALTER TABLE tickets_project ADD COLUMN IF NOT EXISTS ai_kick_enabled     boolean      NOT NULL DEFAULT false;
ALTER TABLE tickets_project ADD COLUMN IF NOT EXISTS ai_kick_repo        varchar(200) NOT NULL DEFAULT '';
ALTER TABLE tickets_project ADD COLUMN IF NOT EXISTS ai_kick_base_branch varchar(100) NOT NULL DEFAULT 'develop';
