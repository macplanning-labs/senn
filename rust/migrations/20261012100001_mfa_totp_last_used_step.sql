-- 二要素認証: 使用済みのコードを覚える(DEMO-000153 A-3)。
-- 同じ確認コードは、有効な間(前後 1 区間 = 約 90 秒)に何度でも使えた。使えた最後の区間(UNIX 時間 / 30 秒)を持ち、
-- それ以前の区間のコードは断る。NULL は「まだ使っていない」。
ALTER TABLE public.mfa_totp_device ADD COLUMN IF NOT EXISTS last_used_step bigint;
