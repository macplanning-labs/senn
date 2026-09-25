-- Local-first 同期の土台（WIPAPPDEV-000114 / 000115 / 000117）
-- 設計: docs/詳細設計書_LocalFirst_コアエンティティ移行.md §2.1〜§2.2, §2.5
--
-- 1. Columns for sync tracking and idempotency
-- 2. Tombstone table for deletion records
-- 3. Trigger functions for automatic sync_changed_at/updated_at updates
-- 4. Trigger functions for display name changes (label name, team name, etc.)
-- 5. Guard function modification for archived team sync-only updates
-- 6. Deletion recording triggers

-- (a) Add sync and idempotency columns
ALTER TABLE tickets_ticket  ADD COLUMN IF NOT EXISTS sync_changed_at   TIMESTAMPTZ NOT NULL DEFAULT NOW();
ALTER TABLE tickets_ticket  ADD COLUMN IF NOT EXISTS client_request_id UUID;
ALTER TABLE tickets_project ADD COLUMN IF NOT EXISTS updated_at        TIMESTAMPTZ;
UPDATE tickets_project SET updated_at = created_at WHERE updated_at IS NULL;
ALTER TABLE tickets_project ALTER COLUMN updated_at SET DEFAULT NOW();
ALTER TABLE tickets_project ALTER COLUMN updated_at SET NOT NULL;
ALTER TABLE tickets_project ADD COLUMN IF NOT EXISTS sync_changed_at   TIMESTAMPTZ NOT NULL DEFAULT NOW();
ALTER TABLE tickets_project ADD COLUMN IF NOT EXISTS client_request_id UUID;
CREATE UNIQUE INDEX IF NOT EXISTS tickets_ticket_client_request_id_uniq  ON tickets_ticket  (client_request_id) WHERE client_request_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS tickets_project_client_request_id_uniq ON tickets_project (client_request_id) WHERE client_request_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS tickets_ticket_sync_idx  ON tickets_ticket  (sync_changed_at, id);
CREATE INDEX IF NOT EXISTS tickets_project_sync_idx ON tickets_project (sync_changed_at, id);

-- (b) Tombstone table for recording deletions
CREATE TABLE IF NOT EXISTS sync_tombstones (
    id          BIGSERIAL PRIMARY KEY,
    entity      VARCHAR(20)  NOT NULL,          -- 'ticket' | 'project'
    entity_id   BIGINT       NOT NULL,
    entity_key  VARCHAR(50),                    -- ticket_key / project prefix
    team_id     BIGINT,
    project_id  BIGINT,
    deleted_at  TIMESTAMPTZ  NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX IF NOT EXISTS sync_tombstones_entity_deleted_idx ON sync_tombstones (entity, deleted_at, id);

-- (1) Automatic updated_at and sync_changed_at for tickets_ticket and tickets_project
CREATE OR REPLACE FUNCTION sync_touch_row() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'UPDATE'
       AND NEW.updated_at IS NOT DISTINCT FROM OLD.updated_at
       AND (to_jsonb(NEW) - 'updated_at' - 'sync_changed_at')
           IS DISTINCT FROM (to_jsonb(OLD) - 'updated_at' - 'sync_changed_at') THEN
        NEW.updated_at := NOW();
    END IF;
    NEW.sync_changed_at := clock_timestamp();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_touch_project ON tickets_project;
CREATE TRIGGER trg_sync_touch_project
    BEFORE INSERT OR UPDATE ON tickets_project
    FOR EACH ROW EXECUTE FUNCTION sync_touch_row();

DROP TRIGGER IF EXISTS trg_sync_touch_ticket ON tickets_ticket;
CREATE TRIGGER trg_sync_touch_ticket
    BEFORE INSERT OR UPDATE ON tickets_ticket
    FOR EACH ROW EXECUTE FUNCTION sync_touch_row();

-- (2) Update parent's updated_at when intermediate tables change
CREATE OR REPLACE FUNCTION sync_touch_ticket_from_link() RETURNS trigger AS $$
DECLARE
    v_id BIGINT;
BEGIN
    v_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.ticketmodel_id ELSE NEW.ticketmodel_id END;
    UPDATE tickets_ticket SET updated_at = NOW() WHERE id = v_id AND NOT team_is_archived(team_id);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_touch_ticket_assignees ON tickets_ticket_assignees;
CREATE TRIGGER trg_sync_touch_ticket_assignees
    AFTER INSERT OR UPDATE OR DELETE ON tickets_ticket_assignees
    FOR EACH ROW EXECUTE FUNCTION sync_touch_ticket_from_link();

DROP TRIGGER IF EXISTS trg_sync_touch_ticket_reviewers ON tickets_ticket_reviewers;
CREATE TRIGGER trg_sync_touch_ticket_reviewers
    AFTER INSERT OR UPDATE OR DELETE ON tickets_ticket_reviewers
    FOR EACH ROW EXECUTE FUNCTION sync_touch_ticket_from_link();

DROP TRIGGER IF EXISTS trg_sync_touch_ticket_labels ON tickets_ticket_labels;
CREATE TRIGGER trg_sync_touch_ticket_labels
    AFTER INSERT OR UPDATE OR DELETE ON tickets_ticket_labels
    FOR EACH ROW EXECUTE FUNCTION sync_touch_ticket_from_link();

CREATE OR REPLACE FUNCTION sync_touch_project_from_link() RETURNS trigger AS $$
DECLARE
    v_id BIGINT;
BEGIN
    v_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.project_id ELSE NEW.project_id END;
    UPDATE tickets_project SET updated_at = NOW() WHERE id = v_id;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_touch_project_teams ON tickets_project_teams;
CREATE TRIGGER trg_sync_touch_project_teams
    AFTER INSERT OR UPDATE OR DELETE ON tickets_project_teams
    FOR EACH ROW EXECUTE FUNCTION sync_touch_project_from_link();

DROP TRIGGER IF EXISTS trg_sync_touch_roadmap_projects ON roadmap_projects;
CREATE TRIGGER trg_sync_touch_roadmap_projects
    AFTER INSERT OR UPDATE OR DELETE ON roadmap_projects
    FOR EACH ROW EXECUTE FUNCTION sync_touch_project_from_link();

-- (3) Update sync_changed_at when display name masters change
CREATE OR REPLACE FUNCTION sync_bump_from_label() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_ticket SET sync_changed_at = clock_timestamp()
    WHERE id IN (SELECT ticketmodel_id FROM tickets_ticket_labels WHERE labelmodel_id = NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_label ON m_label;
CREATE TRIGGER trg_sync_bump_label
    AFTER UPDATE OF name, color, description, category, is_ai_enabled ON m_label
    FOR EACH ROW WHEN (OLD.name IS DISTINCT FROM NEW.name OR OLD.color IS DISTINCT FROM NEW.color OR OLD.description IS DISTINCT FROM NEW.description OR OLD.category IS DISTINCT FROM NEW.category OR OLD.is_ai_enabled IS DISTINCT FROM NEW.is_ai_enabled)
    EXECUTE FUNCTION sync_bump_from_label();

CREATE OR REPLACE FUNCTION sync_bump_from_team() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_ticket SET sync_changed_at = clock_timestamp()
    WHERE team_id = NEW.id;
    UPDATE tickets_project SET sync_changed_at = clock_timestamp()
    WHERE id IN (SELECT project_id FROM tickets_project_teams WHERE team_id = NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_team ON m_team;
CREATE TRIGGER trg_sync_bump_team
    AFTER UPDATE OF name, slug, icon, color, archived_at ON m_team
    FOR EACH ROW WHEN (OLD.name IS DISTINCT FROM NEW.name OR OLD.slug IS DISTINCT FROM NEW.slug OR OLD.icon IS DISTINCT FROM NEW.icon OR OLD.color IS DISTINCT FROM NEW.color OR OLD.archived_at IS DISTINCT FROM NEW.archived_at)
    EXECUTE FUNCTION sync_bump_from_team();

CREATE OR REPLACE FUNCTION sync_bump_from_cycle() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_ticket SET sync_changed_at = clock_timestamp()
    WHERE cycle_id = NEW.id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_cycle ON t_cycle;
CREATE TRIGGER trg_sync_bump_cycle
    AFTER UPDATE OF name ON t_cycle
    FOR EACH ROW WHEN (OLD.name IS DISTINCT FROM NEW.name)
    EXECUTE FUNCTION sync_bump_from_cycle();

CREATE OR REPLACE FUNCTION sync_bump_from_milestone() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_ticket SET sync_changed_at = clock_timestamp()
    WHERE milestone_id = NEW.id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_milestone ON milestones_milestone;
CREATE TRIGGER trg_sync_bump_milestone
    AFTER UPDATE OF name, due_date, description ON milestones_milestone
    FOR EACH ROW WHEN (OLD.name IS DISTINCT FROM NEW.name OR OLD.due_date IS DISTINCT FROM NEW.due_date OR OLD.description IS DISTINCT FROM NEW.description)
    EXECUTE FUNCTION sync_bump_from_milestone();

CREATE OR REPLACE FUNCTION sync_bump_from_category() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_ticket SET sync_changed_at = clock_timestamp()
    WHERE category_id = NEW.id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_category ON tickets_category;
CREATE TRIGGER trg_sync_bump_category
    AFTER UPDATE OF name, slug, color, level, parent_id, sort_order ON tickets_category
    FOR EACH ROW WHEN (OLD.name IS DISTINCT FROM NEW.name OR OLD.slug IS DISTINCT FROM NEW.slug OR OLD.color IS DISTINCT FROM NEW.color OR OLD.level IS DISTINCT FROM NEW.level OR OLD.parent_id IS DISTINCT FROM NEW.parent_id OR OLD.sort_order IS DISTINCT FROM NEW.sort_order)
    EXECUTE FUNCTION sync_bump_from_category();

CREATE OR REPLACE FUNCTION sync_bump_from_project() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_ticket SET sync_changed_at = clock_timestamp()
    WHERE project_id = NEW.id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_project ON tickets_project;
CREATE TRIGGER trg_sync_bump_project
    AFTER UPDATE OF name, prefix ON tickets_project
    FOR EACH ROW WHEN (OLD.name IS DISTINCT FROM NEW.name OR OLD.prefix IS DISTINCT FROM NEW.prefix)
    EXECUTE FUNCTION sync_bump_from_project();

CREATE OR REPLACE FUNCTION sync_bump_from_user() RETURNS trigger AS $$
BEGIN
    UPDATE tickets_ticket SET sync_changed_at = clock_timestamp()
    WHERE author_id = NEW.id OR id IN (
        SELECT ticketmodel_id FROM tickets_ticket_assignees WHERE user_id = NEW.id
    ) OR id IN (
        SELECT ticketmodel_id FROM tickets_ticket_reviewers WHERE user_id = NEW.id
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_bump_user ON accounts_user;
CREATE TRIGGER trg_sync_bump_user
    AFTER UPDATE OF username, email, display_name ON accounts_user
    FOR EACH ROW WHEN (OLD.username IS DISTINCT FROM NEW.username OR OLD.email IS DISTINCT FROM NEW.email OR OLD.display_name IS DISTINCT FROM NEW.display_name)
    EXECUTE FUNCTION sync_bump_from_user();

-- (4) Modify guard_archived_team_row to allow sync_changed_at-only updates
CREATE OR REPLACE FUNCTION guard_archived_team_row()
RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF team_is_archived(NEW.team_id) THEN
            RAISE EXCEPTION 'team_archived: team_id=%', NEW.team_id USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    ELSIF TG_OP = 'UPDATE' THEN
        IF (to_jsonb(NEW) - 'sync_changed_at') = (to_jsonb(OLD) - 'sync_changed_at') THEN
            RETURN NEW;
        END IF;
        IF team_is_archived(OLD.team_id) OR team_is_archived(NEW.team_id) THEN
            RAISE EXCEPTION 'team_archived: team_id=%', COALESCE(NEW.team_id, OLD.team_id)
                USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    ELSE
        IF team_is_archived(OLD.team_id) THEN
            RAISE EXCEPTION 'team_archived: team_id=%', OLD.team_id USING ERRCODE = '55000';
        END IF;
        RETURN OLD;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- (5) Record deletions to sync_tombstones
CREATE OR REPLACE FUNCTION sync_record_tombstone_ticket() RETURNS trigger AS $$
BEGIN
    INSERT INTO sync_tombstones (entity, entity_id, entity_key, team_id, project_id)
    VALUES ('ticket', OLD.id, OLD.ticket_key, OLD.team_id, OLD.project_id);
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_record_tombstone_ticket ON tickets_ticket;
CREATE TRIGGER trg_sync_record_tombstone_ticket
    AFTER DELETE ON tickets_ticket
    FOR EACH ROW EXECUTE FUNCTION sync_record_tombstone_ticket();

CREATE OR REPLACE FUNCTION sync_record_tombstone_project() RETURNS trigger AS $$
BEGIN
    INSERT INTO sync_tombstones (entity, entity_id, entity_key, team_id, project_id)
    VALUES ('project', OLD.id, OLD.prefix, NULL, OLD.id);
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_record_tombstone_project ON tickets_project;
CREATE TRIGGER trg_sync_record_tombstone_project
    AFTER DELETE ON tickets_project
    FOR EACH ROW EXECUTE FUNCTION sync_record_tombstone_project();
