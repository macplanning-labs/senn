-- Phase 3: プロジェクトの親子・関連・ロードマップ(WIPAPPDEV-000066)
-- 設計: docs/詳細設計書_プロジェクト詳細タブ.md §9(追加のみ。既存データは変更しない)

-- 1. 親子(自己参照。子を持つ親は削除できない)
ALTER TABLE tickets_project
    ADD COLUMN parent_project_id BIGINT NULL
        REFERENCES tickets_project(id) ON DELETE RESTRICT,
    ADD CONSTRAINT tickets_project_parent_not_self
        CHECK (parent_project_id IS NULL OR parent_project_id <> id);

CREATE INDEX tickets_project_parent_idx
    ON tickets_project (parent_project_id)
    WHERE parent_project_id IS NOT NULL;

-- 2. 関連(対称。project_id < related_project_id に正規化して1行だけ持つ。自己関連も同時に排除)
CREATE TABLE project_relations (
    id                 BIGSERIAL PRIMARY KEY,
    project_id         BIGINT NOT NULL REFERENCES tickets_project(id) ON DELETE CASCADE,
    related_project_id BIGINT NOT NULL REFERENCES tickets_project(id) ON DELETE CASCADE,
    relation_type      VARCHAR(20) NOT NULL DEFAULT 'related',
    created_by         BIGINT NULL REFERENCES accounts_user(id) ON DELETE SET NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT project_relations_type_chk CHECK (relation_type IN ('related')),
    CONSTRAINT project_relations_order_chk CHECK (project_id < related_project_id),
    CONSTRAINT project_relations_unique UNIQUE (project_id, related_project_id, relation_type)
);

CREATE INDEX project_relations_related_idx ON project_relations (related_project_id);

-- 3. ロードマップ(全社共通)
CREATE TABLE roadmaps (
    id          BIGSERIAL PRIMARY KEY,
    name        VARCHAR(100) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    owner_id    BIGINT NULL REFERENCES accounts_user(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX roadmaps_name_unique ON roadmaps (LOWER(name));

CREATE TABLE roadmap_projects (
    roadmap_id BIGINT NOT NULL REFERENCES roadmaps(id) ON DELETE CASCADE,
    project_id BIGINT NOT NULL REFERENCES tickets_project(id) ON DELETE CASCADE,
    added_by   BIGINT NULL REFERENCES accounts_user(id) ON DELETE SET NULL,
    added_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (roadmap_id, project_id)
);

CREATE INDEX roadmap_projects_project_idx ON roadmap_projects (project_id);

-- 4. 親子の循環をDBで拒否する(アプリ層の検査をすり抜ける経路・同時実行への最終防衛線)
--    新しい親から上向きに祖先をたどり、自分に到達したら循環。
--    親子の変更は advisory lock で直列化する(READ COMMITTED で A→B と B→A が同時に通るのを防ぐ)。
CREATE FUNCTION guard_project_parent_cycle() RETURNS trigger AS $$
BEGIN
    IF NEW.parent_project_id IS NULL THEN
        RETURN NEW;
    END IF;

    PERFORM pg_advisory_xact_lock(hashtext('tickets_project_hierarchy'));

    IF EXISTS (
        WITH RECURSIVE up(id, path) AS (
            SELECT NEW.parent_project_id, ARRAY[NEW.id, NEW.parent_project_id]
            UNION ALL
            SELECT p.parent_project_id, up.path || p.parent_project_id
            FROM tickets_project p
            JOIN up ON p.id = up.id
            WHERE p.parent_project_id IS NOT NULL
              AND p.parent_project_id <> ALL (up.path[2:])
              AND array_length(up.path, 1) < 50
        )
        SELECT 1 FROM up WHERE up.id = NEW.id
    ) THEN
        RAISE EXCEPTION 'project_hierarchy_cycle: project_id=% parent_project_id=%',
            NEW.id, NEW.parent_project_id;
    END IF;

    RETURN NEW;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_project_parent_cycle
    BEFORE INSERT OR UPDATE OF parent_project_id ON tickets_project
    FOR EACH ROW EXECUTE FUNCTION guard_project_parent_cycle();
