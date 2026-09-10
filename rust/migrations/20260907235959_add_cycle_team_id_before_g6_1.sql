-- 20260908000001(G6-1)のUPDATE文とpartial unique indexがt_cycle.team_idを参照するが、
-- この列は本来20260908110001で初めて追加される想定であり、マイグレーション適用順序が
-- 矛盾している(新規DB/CIのクリーンDBでは「column c.team_id does not exist」で失敗する)。
--
-- 20260908000001・20260908110001は既にコミット済み(20260908110001はstaging環境にも
-- 適用済み)のため内容は変更せず、本ファイルでは20260908000001が実行される前の時点で
-- t_cycle.team_id を先行して用意する。列定義は20260908110001が追加するものと同一にする。
--
-- 既にt_cycle.team_idが存在する環境(20260908110001適用済みのDBなど)ではno-op。
ALTER TABLE t_cycle
ADD COLUMN IF NOT EXISTS team_id BIGINT REFERENCES m_team(id) ON DELETE SET NULL;
