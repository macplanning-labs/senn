-- リアルタイム同期(Delta Push)の土台（DEMO-000158）
-- 設計: docs/詳細設計書_リアルタイム同期_DeltaPush.md §6.1, §7.1, §5.4
--
-- 1. 行ごとの版番号 sync_version（先祖返り防止 LWW の比較値）
-- 2. 削除記録にも版番号を持たせる
-- 3. 変更を配信役へ知らせるトリガー（pg_notify。本文は id・版・所属だけ。実データは入れない）
-- 4. WebSocket 接続用の使い捨てトークン表

-- (1) 版番号
ALTER TABLE tickets_ticket  ADD COLUMN IF NOT EXISTS sync_version BIGINT NOT NULL DEFAULT 1;
ALTER TABLE tickets_project ADD COLUMN IF NOT EXISTS sync_version BIGINT NOT NULL DEFAULT 1;
-- 既存の削除記録は版 0（どの upsert より古い）
ALTER TABLE sync_tombstones ADD COLUMN IF NOT EXISTS sync_version BIGINT NOT NULL DEFAULT 0;

-- 既存の sync_touch_row に「UPDATE のたびに版を +1」を足す。
-- 比較（内容が変わったか）の後で加算する。加算前の NEW は OLD と同じ版なので比較に影響しない。
-- アーカイブ済みチームのガード（trg_guard_archived_team_ticket）は名前順でこのトリガーより先に走り、
-- 加算前の NEW を見るため、ガード側の変更は要らない。
CREATE OR REPLACE FUNCTION sync_touch_row() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'UPDATE'
       AND NEW.updated_at IS NOT DISTINCT FROM OLD.updated_at
       AND (to_jsonb(NEW) - 'updated_at' - 'sync_changed_at')
           IS DISTINCT FROM (to_jsonb(OLD) - 'updated_at' - 'sync_changed_at') THEN
        NEW.updated_at := NOW();
    END IF;
    NEW.sync_changed_at := clock_timestamp();
    IF TG_OP = 'UPDATE' THEN
        NEW.sync_version := OLD.sync_version + 1;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- (2) 削除記録に版（削除直前の版 + 1）を残す
CREATE OR REPLACE FUNCTION sync_record_tombstone_ticket() RETURNS trigger AS $$
BEGIN
    INSERT INTO sync_tombstones (entity, entity_id, entity_key, team_id, project_id, sync_version)
    VALUES ('ticket', OLD.id, OLD.ticket_key, OLD.team_id, OLD.project_id, OLD.sync_version + 1);
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sync_record_tombstone_project() RETURNS trigger AS $$
BEGIN
    INSERT INTO sync_tombstones (entity, entity_id, entity_key, team_id, project_id, sync_version)
    VALUES ('project', OLD.id, OLD.prefix, NULL, OLD.id, OLD.sync_version + 1);
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

-- (3) 配信役への通知
-- 通知チャンネル: senn_sync。本文（JSON）:
--   e=種別 a=insert|update|delete|evict id v=版 t=チーム p=プロジェクト
--   ot=移動前のチーム opj=移動前のプロジェクト r=client_request_id
-- 本文は 8000 バイト制限があるので実データは入れない。
-- NOTIFY はコミット時に、コミット順で配送される。ロールバックされた変更は届かない。
CREATE OR REPLACE FUNCTION sync_notify_ticket() RETURNS trigger AS $$
DECLARE
    r RECORD;
    ver BIGINT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        r := OLD;
        ver := OLD.sync_version + 1;
    ELSE
        r := NEW;
        ver := NEW.sync_version;
    END IF;
    PERFORM pg_notify('senn_sync', json_build_object(
        'e', 'ticket',
        'a', lower(TG_OP),
        'id', r.id,
        'v', ver,
        't', r.team_id,
        'p', r.project_id,
        'ot', CASE WHEN TG_OP = 'UPDATE' AND OLD.team_id IS DISTINCT FROM NEW.team_id THEN OLD.team_id END,
        'opj', CASE WHEN TG_OP = 'UPDATE' AND OLD.project_id IS DISTINCT FROM NEW.project_id THEN OLD.project_id END,
        'r', r.client_request_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_ticket ON tickets_ticket;
CREATE TRIGGER trg_sync_notify_ticket
    AFTER INSERT OR UPDATE OR DELETE ON tickets_ticket
    FOR EACH ROW EXECUTE FUNCTION sync_notify_ticket();

-- プロジェクトは全員が見られ、DTO に利用者ごとに変わる項目（is_member 等）があるため、
-- 実データではなく「再取得の合図」として流す（配信役側で stale にする）。
CREATE OR REPLACE FUNCTION sync_notify_project() RETURNS trigger AS $$
DECLARE
    r RECORD;
    ver BIGINT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        r := OLD;
        ver := OLD.sync_version + 1;
    ELSE
        r := NEW;
        ver := NEW.sync_version;
    END IF;
    PERFORM pg_notify('senn_sync', json_build_object(
        'e', 'project',
        'a', lower(TG_OP),
        'id', r.id,
        'v', ver,
        'r', r.client_request_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_project ON tickets_project;
CREATE TRIGGER trg_sync_notify_project
    AFTER INSERT OR UPDATE OR DELETE ON tickets_project
    FOR EACH ROW EXECUTE FUNCTION sync_notify_project();

-- 参加・脱退（権限の変化）を配信役へ知らせる。接続中の利用者の購読を計算し直す
CREATE OR REPLACE FUNCTION sync_notify_access() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('senn_access',
        (CASE WHEN TG_OP = 'DELETE' THEN OLD.user_id ELSE NEW.user_id END)::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_access ON t_team_membership;
CREATE TRIGGER trg_sync_notify_access
    AFTER INSERT OR UPDATE OR DELETE ON t_team_membership
    FOR EACH ROW EXECUTE FUNCTION sync_notify_access();

-- (4) WebSocket 接続用の使い捨てトークン
-- 生のトークンは保存しない（SHA-256）。接続時に DELETE ... RETURNING で1回だけ取り出せる。
CREATE TABLE IF NOT EXISTS realtime_connect_tokens (
    token_hash  BYTEA       PRIMARY KEY,
    user_id     INT         NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS realtime_connect_tokens_expires_idx ON realtime_connect_tokens (expires_at);
