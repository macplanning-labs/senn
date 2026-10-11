-- AI 自動実行: AI が作業してよいリポジトリの一覧(システム管理者が決める)の初期値。
-- プロジェクトのオーナーは、この一覧の中からしかリポジトリを選べない(ほかのチームのリポジトリを指定させない)。
--
-- 既存の環境では、今プロジェクトに設定されているリポジトリを一覧に入れる(今の動きを変えない)。
-- 設定されたプロジェクトが無い環境(新しく立てた環境など)では作らない(=空。管理者が決めるまで、どのリポジトリでも起動できない)。
INSERT INTO system_settings (key, value, updated_at)
SELECT 'sys.ai.kick.allowed_repos',
       json_agg(DISTINCT trim(ai_kick_repo) ORDER BY trim(ai_kick_repo))::text,
       NOW()
FROM tickets_project
WHERE trim(ai_kick_repo) <> ''
HAVING count(*) > 0
ON CONFLICT (key) DO NOTHING;
