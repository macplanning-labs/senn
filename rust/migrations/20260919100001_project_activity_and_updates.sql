-- プロジェクト詳細 Phase 2: Activity(変更履歴・監査ログ) と 進捗報告(Project Updates)
-- 設計: docs/詳細設計書_プロジェクト詳細タブ.md
--
-- 1. project_activity : プロジェクトで起きた出来事の時系列ログ(追記のみ)
-- 2. project_updates  : 進捗報告(On track / At risk / Off track)。監査ログとは別の構造化データ
-- 3. トリガー         : チケットの追加・除外・完了を、書き込み経路(API/AI Agent/Git連携)によらず記録

-- ---------------------------------------------------------------------------
-- 1. project_activity
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS project_activity (
    id          BIGSERIAL PRIMARY KEY,
    project_id  BIGINT      NOT NULL REFERENCES tickets_project(id) ON DELETE CASCADE,
    -- 実行者。トリガー経由(チケット系イベント)は特定できないため NULL を許す
    actor_id    BIGINT      REFERENCES accounts_user(id) ON DELETE SET NULL,
    event_type  VARCHAR(40) NOT NULL,
    payload     JSONB       NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 一覧取得(新しい順・id をカーソルとするページング)用。id は採番順=発生順
CREATE INDEX IF NOT EXISTS idx_project_activity_project_id_desc
    ON project_activity (project_id, id DESC);

-- ---------------------------------------------------------------------------
-- 2. project_updates
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS project_updates (
    id          BIGSERIAL PRIMARY KEY,
    project_id  BIGINT      NOT NULL REFERENCES tickets_project(id) ON DELETE CASCADE,
    author_id   BIGINT      REFERENCES accounts_user(id) ON DELETE SET NULL,
    health      VARCHAR(20) NOT NULL,
    body        TEXT        NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT project_updates_health_check
        CHECK (health IN ('on_track', 'at_risk', 'off_track'))
);

CREATE INDEX IF NOT EXISTS idx_project_updates_project_id_desc
    ON project_updates (project_id, id DESC);

-- ---------------------------------------------------------------------------
-- 3. チケット系イベントのトリガー
--    ticket_added / ticket_removed / ticket_completed
--    (タイトルはその時点の値を payload に保存する。後でチケットが変わっても履歴は変わらない)
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION project_activity_on_ticket_insert()
RETURNS trigger AS $$
BEGIN
    IF NEW.project_id IS NOT NULL THEN
        INSERT INTO project_activity (project_id, actor_id, event_type, payload)
        VALUES (NEW.project_id, NULL, 'ticket_added',
                jsonb_build_object('ticket_key', NEW.ticket_key, 'title', NEW.title));
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION project_activity_on_ticket_update()
RETURNS trigger AS $$
BEGIN
    -- プロジェクトの付け替え/追加/除外
    IF NEW.project_id IS DISTINCT FROM OLD.project_id THEN
        IF OLD.project_id IS NOT NULL THEN
            INSERT INTO project_activity (project_id, actor_id, event_type, payload)
            VALUES (OLD.project_id, NULL, 'ticket_removed',
                    jsonb_build_object('ticket_key', OLD.ticket_key, 'title', OLD.title));
        END IF;
        IF NEW.project_id IS NOT NULL THEN
            INSERT INTO project_activity (project_id, actor_id, event_type, payload)
            VALUES (NEW.project_id, NULL, 'ticket_added',
                    jsonb_build_object('ticket_key', NEW.ticket_key, 'title', NEW.title));
        END IF;
    END IF;

    -- 完了(closed になった時)
    IF NEW.project_id IS NOT NULL
       AND NEW.status = 'closed'
       AND OLD.status IS DISTINCT FROM 'closed' THEN
        INSERT INTO project_activity (project_id, actor_id, event_type, payload)
        VALUES (NEW.project_id, NULL, 'ticket_completed',
                jsonb_build_object('ticket_key', NEW.ticket_key, 'title', NEW.title));
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_project_activity_ticket_insert ON tickets_ticket;
CREATE TRIGGER trg_project_activity_ticket_insert
    AFTER INSERT ON tickets_ticket
    FOR EACH ROW EXECUTE FUNCTION project_activity_on_ticket_insert();

DROP TRIGGER IF EXISTS trg_project_activity_ticket_update ON tickets_ticket;
CREATE TRIGGER trg_project_activity_ticket_update
    AFTER UPDATE OF project_id, status ON tickets_ticket
    FOR EACH ROW EXECUTE FUNCTION project_activity_on_ticket_update();
