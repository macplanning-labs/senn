-- L1: assigned_team_id（担当チーム）廃止。
-- 個人アサイン(tickets_ticket_assignees)とチケット所属チーム(tickets_ticket.team_id)に一本化する。
-- 単独インデックス tickets_ticket_assigned_team_id_766a4b65 と m_team への FK 制約は列DROPで自動的に削除される。

-- 事前確認（実行結果を完了報告に記録する）
-- SELECT count(*) FROM tickets_ticket WHERE assigned_team_id IS NOT NULL;

ALTER TABLE tickets_ticket DROP COLUMN assigned_team_id;
