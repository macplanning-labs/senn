-- リアルタイム同期 Phase 3: 合図(signal)（DEMO-000158）
-- 設計: docs/詳細設計書_リアルタイム同期_DeltaPush.md §16
--
-- 端末内 DB に表を持たず、REST + react-query で動いている対象（添付・リアクション・サイクル・通知・Wiki）は、
-- 実データではなく「変わった」という合図だけを流す。端末は該当のキャッシュを取り直す（取り直しの権限判定は REST が行う）。
-- 版番号や削除記録は要らない（合図は冪等で、失われても定期同期・再同期で取り直される）。
-- 本文は senn_sync チャンネル。e=種別 a=insert|update|delete id t=チーム p=プロジェクト tid=チケット teams=[所属チーム] u=利用者

-- 添付・リアクション: 親チケットの所属（チーム・プロジェクト）を引く。
-- チケット削除の連鎖でリアクションが消えるときは親が既に無いので送らない（端末は親チケットの削除から子を消す）。
-- TG_ARGV[0] = 種別, TG_ARGV[1] = 'row'（行の id を送る）| 'ticket'（チケットの id を送る。リアクションは「そのチケットのリアクション」）
CREATE OR REPLACE FUNCTION sync_notify_ticket_child() RETURNS trigger AS $$
DECLARE
    r RECORD;
    tm BIGINT;
    pj BIGINT;
BEGIN
    IF TG_OP = 'DELETE' THEN r := OLD; ELSE r := NEW; END IF;
    SELECT team_id, project_id INTO tm, pj FROM tickets_ticket WHERE id = r.ticket_id;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;
    PERFORM pg_notify('senn_sync', json_build_object(
        'e', TG_ARGV[0],
        'a', lower(TG_OP),
        'id', CASE WHEN TG_ARGV[1] = 'ticket' THEN r.ticket_id ELSE r.id END,
        't', tm,
        'p', pj,
        'tid', r.ticket_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_attachment ON tickets_attachment;
CREATE TRIGGER trg_sync_notify_attachment
    AFTER INSERT OR UPDATE OR DELETE ON tickets_attachment
    FOR EACH ROW EXECUTE FUNCTION sync_notify_ticket_child('attachment', 'row');

DROP TRIGGER IF EXISTS trg_sync_notify_reaction ON tickets_reaction;
CREATE TRIGGER trg_sync_notify_reaction
    AFTER INSERT OR UPDATE OR DELETE ON tickets_reaction
    FOR EACH ROW EXECUTE FUNCTION sync_notify_ticket_child('reaction', 'ticket');

-- サイクル: チームに属する（プロジェクトは任意）。チケットと同じ「チーム + プロジェクト」で宛先を決める
CREATE OR REPLACE FUNCTION sync_notify_cycle() RETURNS trigger AS $$
DECLARE
    r RECORD;
BEGIN
    IF TG_OP = 'DELETE' THEN r := OLD; ELSE r := NEW; END IF;
    PERFORM pg_notify('senn_sync', json_build_object(
        'e', 'cycle',
        'a', lower(TG_OP),
        'id', r.id,
        't', r.team_id,
        'p', r.project_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_cycle ON t_cycle;
CREATE TRIGGER trg_sync_notify_cycle
    AFTER INSERT OR UPDATE OR DELETE ON t_cycle
    FOR EACH ROW EXECUTE FUNCTION sync_notify_cycle();

-- Wiki: チーム指定はチケットと同じ「チーム + プロジェクト」、プロジェクトのみはその所属チーム、どちらも無ければ全員へ
CREATE OR REPLACE FUNCTION sync_notify_wiki_page() RETURNS trigger AS $$
DECLARE
    r RECORD;
BEGIN
    IF TG_OP = 'DELETE' THEN r := OLD; ELSE r := NEW; END IF;
    PERFORM pg_notify('senn_sync', json_build_object(
        'e', 'wiki',
        'a', lower(TG_OP),
        'id', r.id,
        't', r.team_id,
        'p', r.project_id,
        'teams', (SELECT COALESCE(json_agg(pt.team_id), '[]'::json) FROM tickets_project_teams pt WHERE pt.project_id = r.project_id)
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_wiki_page ON wiki_page;
CREATE TRIGGER trg_sync_notify_wiki_page
    AFTER INSERT OR UPDATE OR DELETE ON wiki_page
    FOR EACH ROW EXECUTE FUNCTION sync_notify_wiki_page();

-- Wiki の添付: ページの所属を引いて、同じ合図（ページの id）を送る
CREATE OR REPLACE FUNCTION sync_notify_wiki_attachment() RETURNS trigger AS $$
DECLARE
    r RECORD;
    tm BIGINT;
    pj BIGINT;
BEGIN
    IF TG_OP = 'DELETE' THEN r := OLD; ELSE r := NEW; END IF;
    SELECT team_id, project_id INTO tm, pj FROM wiki_page WHERE id = r.wiki_page_id;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;
    PERFORM pg_notify('senn_sync', json_build_object(
        'e', 'wiki',
        'a', lower(TG_OP),
        'id', r.wiki_page_id,
        't', tm,
        'p', pj,
        'teams', (SELECT COALESCE(json_agg(pt.team_id), '[]'::json) FROM tickets_project_teams pt WHERE pt.project_id = pj)
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_wiki_attachment ON wiki_attachment;
CREATE TRIGGER trg_sync_notify_wiki_attachment
    AFTER INSERT OR UPDATE OR DELETE ON wiki_attachment
    FOR EACH ROW EXECUTE FUNCTION sync_notify_wiki_attachment();

-- 通知: 本人だけの部屋（u:利用者）へ。既読の更新も送る（別のタブ・端末の未読数を合わせるため）
CREATE OR REPLACE FUNCTION sync_notify_notification() RETURNS trigger AS $$
DECLARE
    r RECORD;
BEGIN
    IF TG_OP = 'DELETE' THEN r := OLD; ELSE r := NEW; END IF;
    PERFORM pg_notify('senn_sync', json_build_object(
        'e', 'notification',
        'a', lower(TG_OP),
        'id', r.id,
        'u', r.user_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_notify_notification ON notifications_notification;
CREATE TRIGGER trg_sync_notify_notification
    AFTER INSERT OR UPDATE OR DELETE ON notifications_notification
    FOR EACH ROW EXECUTE FUNCTION sync_notify_notification();
