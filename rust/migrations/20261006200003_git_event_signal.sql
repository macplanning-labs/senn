-- GitHub 連携 Phase 2: コミット・PR の一覧(t_git_event)を、リアルタイム同期の「合図」で知らせる。追加のみ。
-- 設計: docs/design/詳細設計_GitHub連携Phase2.md §6.2、docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §16
--
-- 添付・リアクションと同じ関数(sync_notify_ticket_child)を使う。実データではなく「変わった」という合図だけを流し、
-- 端末は、チケット画面の「GIT ACTIVITY」のキャッシュを取り直す。
DROP TRIGGER IF EXISTS trg_sync_notify_git_event ON t_git_event;
CREATE TRIGGER trg_sync_notify_git_event
    AFTER INSERT OR UPDATE OR DELETE ON t_git_event
    FOR EACH ROW EXECUTE FUNCTION sync_notify_ticket_child('git_event', 'row');
