-- チームのアーカイブ(WIPAPPDEV-000069 第2段階)
-- 設計: docs/詳細設計書_プロジェクト詳細タブ.md「6.6 チームのアーカイブ」
--
-- アーカイブしたチームは「閲覧専用」になる。チケット・サイクル・コメント・添付の
-- 作成/更新/削除を、書き込み経路(画面/AI連携/Git連携/外部連携)によらず、
-- DBのトリガーで拒否する。復元(archived_at を NULL に戻す)すれば、再び編集できる。
--
-- 既存の「有効」(is_active)は、これまでどこにも効いていなかったため使わない。
-- (本番で既に無効にしているチームが、意図せずアーカイブ扱いになるのを避ける)

ALTER TABLE m_team ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;
ALTER TABLE m_team ADD COLUMN IF NOT EXISTS archived_by BIGINT
    REFERENCES accounts_user(id) ON DELETE SET NULL;

-- チームがアーカイブ済みか(NULL・存在しないチームは false)
CREATE OR REPLACE FUNCTION team_is_archived(p_team_id BIGINT)
RETURNS BOOLEAN AS $$
    SELECT COALESCE((SELECT archived_at IS NOT NULL FROM m_team WHERE id = p_team_id), FALSE)
$$ LANGUAGE sql STABLE;

-- tickets_ticket / t_cycle: 自身の team_id で判定する
CREATE OR REPLACE FUNCTION guard_archived_team_row()
RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF team_is_archived(NEW.team_id) THEN
            RAISE EXCEPTION 'team_archived: team_id=%', NEW.team_id USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    ELSIF TG_OP = 'UPDATE' THEN
        -- 元のチームがアーカイブ済み、または、アーカイブ済みのチームへ移す操作も拒否する
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

-- tickets_comment / tickets_attachment: 紐づくチケットのチームで判定する
CREATE OR REPLACE FUNCTION guard_archived_team_via_ticket()
RETURNS trigger AS $$
DECLARE
    v_ticket_id BIGINT;
    v_team_id   BIGINT;
BEGIN
    v_ticket_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.ticket_id ELSE NEW.ticket_id END;
    SELECT team_id INTO v_team_id FROM tickets_ticket WHERE id = v_ticket_id;
    IF team_is_archived(v_team_id) THEN
        RAISE EXCEPTION 'team_archived: team_id=%', v_team_id USING ERRCODE = '55000';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_guard_archived_team_ticket ON tickets_ticket;
CREATE TRIGGER trg_guard_archived_team_ticket
    BEFORE INSERT OR UPDATE OR DELETE ON tickets_ticket
    FOR EACH ROW EXECUTE FUNCTION guard_archived_team_row();

DROP TRIGGER IF EXISTS trg_guard_archived_team_cycle ON t_cycle;
CREATE TRIGGER trg_guard_archived_team_cycle
    BEFORE INSERT OR UPDATE OR DELETE ON t_cycle
    FOR EACH ROW EXECUTE FUNCTION guard_archived_team_row();

DROP TRIGGER IF EXISTS trg_guard_archived_team_comment ON tickets_comment;
CREATE TRIGGER trg_guard_archived_team_comment
    BEFORE INSERT OR UPDATE OR DELETE ON tickets_comment
    FOR EACH ROW EXECUTE FUNCTION guard_archived_team_via_ticket();

DROP TRIGGER IF EXISTS trg_guard_archived_team_attachment ON tickets_attachment;
CREATE TRIGGER trg_guard_archived_team_attachment
    BEFORE INSERT OR UPDATE OR DELETE ON tickets_attachment
    FOR EACH ROW EXECUTE FUNCTION guard_archived_team_via_ticket();
