-- ============================================================
-- JWT リフレッシュトークン ブラックリスト(Rust側で自己完結)
--
-- Djangoのrest_framework_simplejwt.token_blacklistアプリはINSTALLED_APPSに
-- 含まれておらず(config/settings.py確認済み)、Django側のブラックリスト機能は
-- 実際には動作していない(SIMPLE_JWT.BLACKLIST_AFTER_ROTATION=Trueだが無効)。
-- RustはDjangoのテーブル形式を踏襲せず、jti(JWT ID)だけを持つ
-- 独立したブラックリストテーブルを自己完結で持つ。
-- ============================================================
CREATE TABLE jwt_blacklisted_token (
    jti VARCHAR(64) PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_jwt_blacklist_expires ON jwt_blacklisted_token (expires_at);
