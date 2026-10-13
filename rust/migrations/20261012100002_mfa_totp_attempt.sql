-- 二要素認証: ユーザー単位の試行の数え方を DB に保存する(DEMO-000153 A-1・A-2)。
-- ログインし直して MFA トークンを取り直しても、数は引き継がれる。再起動・複数台でも消えず、合算される。
-- attempts は、時間窓(window_start から)の間に「確認コードを調べた回数」。locked_until が未来なら、その人の確認コードは止まっている。
CREATE TABLE IF NOT EXISTS public.mfa_totp_attempt (
    user_id      bigint PRIMARY KEY,
    attempts     integer NOT NULL DEFAULT 0,
    window_start timestamp with time zone NOT NULL DEFAULT now(),
    locked_until timestamp with time zone
);

-- 期限切れの行を消すときに、古い順にたどるための索引(ロック中の行は除く)
CREATE INDEX IF NOT EXISTS mfa_totp_attempt_window_idx
    ON public.mfa_totp_attempt (window_start)
    WHERE locked_until IS NULL;
