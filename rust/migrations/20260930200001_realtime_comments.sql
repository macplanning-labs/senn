-- リアルタイム同期 Phase 2: コメント（DEMO-000158）
-- 設計: docs/詳細設計書_リアルタイム同期_DeltaPush.md §9
--
-- 1. コメントの版番号・同期用の時刻
-- 2. 変更通知（pg_notify）。行の所属（チーム・プロジェクト）は親チケットから引く
-- 3. 削除記録（物理削除。通常の削除は deleted_at による論理削除で、通常の更新として流れる）
-- 4. コメントの追加・削除でチケット行の版も進める（一覧のコメント数が変わるため）
-- 5. チケットのチーム/プロジェクト移動でコメントの版も進める（移動先の端末にコメントを届けるため）

-- (1)
ALTER TABLE tickets_comment ADD COLUMN IF NOT EXISTS sync_version    BIGINT      NOT NULL DEFAULT 1;
ALTER TABLE tickets_comment ADD COLUMN IF NOT EXISTS sync_changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
CREATE INDEX IF NOT EXISTS tickets_comment_sync_idx ON tickets_comment (sync_changed_at, id);

-- アーカイブ済みチームのガード（trg_guard_archived_team_comment）は名前順でこのトリガーより先に走り、
-- 加算前の NEW を見る。
CREATE OR REPLACE FUNCTION sync_touch_comment() RETURNS trigger AS $$
BEGIN
    NEW.sync_changed_at := clock_timestamp();
    IF TG_OP = 'UPDATE' THEN
        NEW.sync_version := OLD.sync_version + 1;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_touch_comment ON tickets_comment;
CREATE TRIGGER trg_sync_touch_comment
    BEFORE INSERT OR UPDATE ON tickets_comment
    FOR EACH ROW EXECUTE FUNCTION sync_touch_comment();

-- (2) 本文は senn_sync チャンネル。チケット削除の連鎖でコメントが消えるときは、親が既に無く部屋が決められない。
-- その場合は送らない（端末は親チケットの削除から子のコメントを消す）。
CREATE OR REPLACE FUNCTION sync_notify_comment() RETURNS trigger AS $$
DECLARE
    r RECORD;
    ver BIGINT;
    tm BIGINT;
    pj BIGINT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        r := OLD;
        ver := OLD.sync_version + 1;
    ELSE
        r := NEW;
        ver := NEW.sync_version;
    END IF;
    SELECT team_id, project_id INTO tm, pj FROM tickets_ticket WHERE id = r.ticket_id;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;
    PERFORM pg_notify('senn_sync', json_build_object(
        'e', 'comment',
        'a', lower(TG_OP),
        'id', r.id,
        'v', ver,
        't', tm,
        'p', pj,
        'tid', r.ticket_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_comment ON tickets_comment;
CREATE TRIGGER trg_sync_notify_comment
    AFTER INSERT OR UPDATE OR DELETE ON tickets_comment
    FOR EACH ROW EXECUTE FUNCTION sync_notify_comment();

-- (3) 物理削除の記録。entity_key にコメント ID を入れる（差分同期で「削除」として返すため）
CREATE OR REPLACE FUNCTION sync_record_tombstone_comment() RETURNS trigger AS $$
DECLARE
    tm BIGINT;
    pj BIGINT;
BEGIN
    SELECT team_id, project_id INTO tm, pj FROM tickets_ticket WHERE id = OLD.ticket_id;
    INSERT INTO sync_tombstones (entity, entity_id, entity_key, team_id, project_id, sync_version)
    VALUES ('comment', OLD.id, OLD.id::text, tm, pj, OLD.sync_version + 1);
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_record_tombstone_comment ON tickets_comment;
CREATE TRIGGER trg_sync_record_tombstone_comment
    AFTER DELETE ON tickets_comment
    FOR EACH ROW EXECUTE FUNCTION sync_record_tombstone_comment();

-- (4) チケットのコメント数（一覧の commentCount）は行の外の値なので、追加・削除のたびにチケット行を進める。
-- sync_changed_at だけの更新は、アーカイブ済みチームのガードも通る。
CREATE OR REPLACE FUNCTION sync_bump_ticket_from_comment() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_ticket SET sync_changed_at = clock_timestamp()
    WHERE id = CASE WHEN TG_OP = 'DELETE' THEN OLD.ticket_id ELSE NEW.ticket_id END;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_ticket_from_comment ON tickets_comment;
CREATE TRIGGER trg_sync_bump_ticket_from_comment
    AFTER INSERT OR DELETE ON tickets_comment
    FOR EACH ROW EXECUTE FUNCTION sync_bump_ticket_from_comment();

-- (5) 移動先のチーム/プロジェクトの端末は、これまでそのチケットのコメントを持っていない。
-- コメントの版を進めて、通常の変更として移動先の部屋へ流す。
CREATE OR REPLACE FUNCTION sync_bump_comments_on_ticket_move() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_comment SET sync_changed_at = clock_timestamp() WHERE ticket_id = NEW.id;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_comments_on_ticket_move ON tickets_ticket;
CREATE TRIGGER trg_sync_bump_comments_on_ticket_move
    AFTER UPDATE OF team_id, project_id ON tickets_ticket
    FOR EACH ROW
    WHEN (OLD.team_id IS DISTINCT FROM NEW.team_id OR OLD.project_id IS DISTINCT FROM NEW.project_id)
    EXECUTE FUNCTION sync_bump_comments_on_ticket_move();
