-- 管理者のパスキー必須(DEMO-000153 A-4b): パスワードだけでパスキーを登録できてしまうと、パスワードを知った攻撃者が
-- 自分の鍵を登録して管理者になれる。そのため、強制の期間に「パスワードのログインから、パスキーを新しく登録する」ことは、
-- **別のシステム管理者(または復旧コマンド)が、その人のために窓を開けたとき(パスキーのリセット)だけ**許す。
-- この列は、その窓が開いている期限(NULL = 閉じている)。
ALTER TABLE public.accounts_user ADD COLUMN IF NOT EXISTS passkey_enroll_until timestamp with time zone;
