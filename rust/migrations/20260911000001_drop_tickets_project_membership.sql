-- L2②: 旧 tickets_project_membership を廃止する(破壊的)。
-- 前提: L2①(scoped_project_id付きt_team_membershipへのデータ移行)がステージング実データで検証済み。
-- アプリ側の移行期間中の暫定OR分岐(check_ticket_access, push_ticket_access_sql,
-- dashboard_api_repo.rs, user_repo.rs::find_project_members, ai_agent_api.rs等)は
-- 同じdeployで t_team_membership の1系統に統一済み。
--
-- 事前確認(ステージング実データ、2026-09-11時点): 旧20件 = 新規scoped 10件 + 既存全体アクセスでスキップ 10件、
-- 実データでの境界検証(scoped一致/team不一致/既存全体メンバー)すべて期待通り。
-- 詳細は docs/worklogs/ftfc-wave8-staging-migration-plan.md §4 参照。

DROP TABLE tickets_project_membership;
