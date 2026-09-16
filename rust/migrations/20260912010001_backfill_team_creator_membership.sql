-- 既存チームへの「作成者(オーナー)未登録」問題の遡及対応。
--
-- team_api.rs::team_create が Extension<AuthUser> を `_auth` として握りつぶしており、
-- チーム作成者を t_team_membership に一切登録していなかった(ハンドラ側は別途修正済み、
-- 新規チームは作成時に作成者がadminとして自動登録される)。
-- push_ticket_access_sql は is_staff か t_team_membership 保有者のみアクセスを許可するため、
-- 作成者本人がそのチームで作成した自分のチケットさえ一覧に表示されない状態になっていた
-- (OSS公開チーム, team_id=17 で顕在化。チケット作成自体は201で成功するが一覧が常に空)。
--
-- 対象: t_team_membership を1件も持たないチームのうち、紐づく tickets_project.owner_id が
-- 判明しているものへ、そのownerをadminとして追加する(冪等: 既にメンバーがいるチーム、
-- owner_id 不明のプロジェクトは対象外)。

INSERT INTO t_team_membership (team_id, user_id, role, joined_at)
SELECT DISTINCT p.owner_team_id, p.owner_id, 'admin', NOW()
FROM tickets_project p
WHERE p.owner_team_id IS NOT NULL
  AND p.owner_id IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM t_team_membership tm WHERE tm.team_id = p.owner_team_id
  )
ON CONFLICT DO NOTHING;
