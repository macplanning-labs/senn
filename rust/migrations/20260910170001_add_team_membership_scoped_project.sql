-- L2 ①: Projectゲストアクセス機構の新設(非破壊)。
-- t_team_membership に scoped_project_id を追加し、Project限定ゲスト参加を Team 側の1系統で表現できるようにする。
--
-- レビュー指摘1(2026-09-10、実装時に発覚): 既存の UNIQUE(team_id, user_id) 制約のままだと、
-- 「1ユーザーが同一チーム傘下の複数Projectにゲスト参加する」(詳細設計書§3.3で将来サポートを明記)ができない。
-- PostgreSQL 15+ の UNIQUE NULLS NOT DISTINCT を使い、(team_id, user_id, scoped_project_id) を
-- NULLも「同じ値」として扱う1本の制約に統一する(ユーザー承認、案A)。これにより
-- 「通常メンバー行(scoped_project_id IS NULL)は1人1チーム1行まで」
-- 「ゲスト行(scoped_project_id IS NOT NULL)は (team_id, user_id, scoped_project_id) 単位で複数行可」
-- の両方を1つの制約で表現できる。
--
-- レビュー指摘2(2026-09-10、実装時に発覚): 旧 tickets_project_membership の期限付きアクセス
-- (end_date + tickets_project.grace_period_days による猶予期間判定、membership_repo.rs
-- check_membership_exists / calculate_membership_status)が、新設計のデータ移行では引き継がれず
-- 全員恒久アクセスに格上げされてしまう欠落があった。end_date を t_team_membership にも持たせて
-- 引き継ぎ、アプリ層(check_ticket_access等)で従来通り期限＋猶予期間判定を継続する(ユーザー承認、案A)。
-- grace_period_days はProjectごとの設定のままでよいため複製せず、scoped_project_id 経由で
-- tickets_project を参照して都度引く。end_date は scoped_project_id IS NULL の通常メンバー行では
-- 使わない(NULLのまま=無期限)。

ALTER TABLE t_team_membership
  ADD COLUMN scoped_project_id BIGINT REFERENCES tickets_project(id) ON DELETE CASCADE,
  ADD COLUMN end_date DATE;

ALTER TABLE t_team_membership DROP CONSTRAINT unique_team_user;

ALTER TABLE t_team_membership
  ADD CONSTRAINT unique_team_user_scoped UNIQUE NULLS NOT DISTINCT (team_id, user_id, scoped_project_id);

-- scoped_project_idを持つ行はroleがmember固定であることをアプリ層で担保する(role値の変更余地を残すためCHECK制約は課さない)
CREATE INDEX idx_team_membership_scoped_project ON t_team_membership (scoped_project_id) WHERE scoped_project_id IS NOT NULL;

-- 事前確認(ローカル開発用DBで実施し、結果を完了報告に記録): 2026-09-10時点
-- 移行対象: tickets_project_membership 7件 / 既にTeamメンバーでスキップされる件数: 0件 / owner_team未設定でロストする件数: 0件
-- end_date設定済み行: 0件(ローカル開発用DBのため。ステージング・本番適用時は必ず再確認すること)

-- データ移行: 既存 tickets_project_membership の全行を、対応するTeamへの scoped_project_id・end_date 付き
-- t_team_membership 行として移行する。既に何らかの形(scoped/unscoped問わず)でそのTeamのメンバーなら
-- 追加しない(全体アクセスは限定アクセスを包含するため)。
INSERT INTO t_team_membership (team_id, user_id, role, scoped_project_id, end_date, joined_at)
SELECT p.owner_team_id, pm.user_id, 'member', pm.project_id, pm.end_date, pm.created_at
FROM tickets_project_membership pm
JOIN tickets_project p ON p.id = pm.project_id
WHERE p.owner_team_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM t_team_membership tm
    WHERE tm.team_id = p.owner_team_id AND tm.user_id = pm.user_id
  )
ON CONFLICT DO NOTHING;
