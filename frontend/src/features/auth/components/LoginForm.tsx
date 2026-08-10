/**
 * LoginForm.tsx — ログインフォーム
 *
 * ユーザー名 + パスワードでJWT認証。
 * Zodバリデーション + 入力値保持 + エラー表示。
 */

import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate } from 'react-router-dom';
import { z } from 'zod';
import { startAuthentication } from '@simplewebauthn/browser';
import { useAuthStore } from '@/shared/stores/authStore';
import { apiClient } from '@/shared/api/client';
import './LoginForm.css';

const loginSchema = z.object({
  username: z.string().min(1, 'Username is required'),
  password: z.string().min(1, 'Password is required'),
});

export function LoginForm() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const login = useAuthStore((s) => s.login);
  const verifyMfa = useAuthStore((s) => s.verifyMfa);
  const loginWithPasskey = useAuthStore((s) => s.loginWithPasskey);

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  // MFA（TOTP）2段階目
  const [mfaToken, setMfaToken] = useState<string | null>(null);
  const [totpCode, setTotpCode] = useState('');

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError('');

    // フロント側バリデーション（Layer 1）
    const result = loginSchema.safeParse({ username, password });
    if (!result.success) {
      setError(result.error.issues[0]?.message ?? 'Validation error');
      return;
    }

    setIsLoading(true);
    try {
      const loginResult = await login(username, password);
      if (loginResult.mfaRequired) {
        setMfaToken(loginResult.mfaToken);
        return;
      }
      navigate('/dashboard');
    } catch {
      setError(t('auth.loginError', 'Invalid username or password'));
    } finally {
      setIsLoading(false);
    }
  }

  async function handleMfaSubmit(e: FormEvent) {
    e.preventDefault();
    setError('');
    if (!mfaToken) return;

    setIsLoading(true);
    try {
      await verifyMfa(mfaToken, totpCode);
      navigate('/dashboard');
    } catch {
      setError(t('auth.mfaError', 'Invalid authentication code'));
    } finally {
      setIsLoading(false);
    }
  }

  async function handlePasskeyLogin() {
    setError('');
    setIsLoading(true);
    try {
      const beginRes = await apiClient.post('/auth/passkey/login/begin/');
      const { request_challenge, auth_state_json } = beginRes.data;
      const optionsJSON = request_challenge.publicKey ?? request_challenge;

      const credential = await startAuthentication({ optionsJSON });

      const completeRes = await apiClient.post('/auth/passkey/login/complete/', {
        auth_state_json,
        credential,
      });

      await loginWithPasskey(completeRes.data.access, completeRes.data.refresh);
      navigate('/dashboard');
    } catch (err: any) {
      const msg = err?.message || '';
      if (/AbortError|NotAllowedError|取消|canceled|cancelled/i.test(msg)) {
        // ユーザーが自らキャンセルした場合は、画面全体のエラー表示は
        // 出さない。静かにローディング状態を解除するのみ。
      } else {
        const detail = err?.response?.data?.detail;
        setError(detail || 'パスキー認証に失敗しました');
      }
    } finally {
      setIsLoading(false);
    }
  }

  if (mfaToken) {
    return (
      <div className="login" data-testid="login-page">
        <div className="login__card">
          <div className="login__header">
            <h1 className="login__logo">WIP</h1>
            <p className="login__tagline">{t('auth.mfaPrompt', 'Enter your authenticator code')}</p>
          </div>

          <form
            className="login__form"
            onSubmit={(e) => { void handleMfaSubmit(e); }}
            data-testid="mfa-verify-form"
          >
            {error && (
              <div className="login__error" data-testid="login-error" role="alert">
                {error}
              </div>
            )}

            <div className="login__field">
              <label htmlFor="totp-code" className="login__label">
                {t('auth.totpCode', 'Authentication code')}
              </label>
              <input
                id="totp-code"
                type="text"
                inputMode="numeric"
                autoComplete="one-time-code"
                className="login__input"
                value={totpCode}
                onChange={(e) => setTotpCode(e.target.value)}
                autoFocus
                data-testid="mfa-code-input"
              />
            </div>

            <button
              type="submit"
              className="login__submit"
              disabled={isLoading}
              data-testid="mfa-verify-submit"
            >
              {isLoading ? t('common.loading') : t('auth.verify', 'Verify')}
            </button>
          </form>
        </div>
      </div>
    );
  }

  return (
    <div className="login" data-testid="login-page">
      <div className="login__card">
        {/* ロゴ */}
        <div className="login__header">
          <h1 className="login__logo">WIP</h1>
          <p className="login__tagline">{t('app.tagline')}</p>
        </div>

        {/* フォーム */}
        <form className="login__form" onSubmit={(e) => { void handleSubmit(e); }} data-testid="login-form">
          {error && (
            <div className="login__error" data-testid="login-error" role="alert">
              {error}
            </div>
          )}

          <div className="login__field">
            <label htmlFor="username" className="login__label">
              {t('auth.username', 'Username')}
            </label>
            <input
              id="username"
              type="text"
              name="username"
              className="login__input"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="admin"
              autoComplete="username"
              autoFocus
              data-testid="login-username"
            />
          </div>

          <div className="login__field">
            <div className="login__label-row">
              <label htmlFor="password" className="login__label">
                {t('auth.password')}
              </label>
              <Link to="/forgot-password" className="login__forgot" tabIndex={-1}>
                {t('auth.forgotPassword')}
              </Link>
            </div>
            <input
              id="password"
              type="password"
              className="login__input"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="current-password"
              data-testid="login-password"
            />
          </div>

          <button
            type="submit"
            className="login__submit"
            disabled={isLoading}
            data-testid="login-submit"
          >
            {isLoading ? t('common.loading') : t('auth.login')}
          </button>
        </form>

        {/* パスキーログイン区切り */}
        <div className="login__divider">
          <span>{t('auth.or', 'or')}</span>
        </div>

        {/* パスキーログインボタン */}
        <button
          type="button"
          className="login__passkey-btn"
          onClick={() => { void handlePasskeyLogin(); }}
          disabled={isLoading}
        >
          🔑 {t('auth.loginWithPasskey', 'Sign in with Passkey')}
        </button>

        {/* サインアップリンク */}
        <p className="login__signup">
          {t('auth.noAccount', "Don't have an account?")}{' '}
          <Link to="/register" data-testid="register-link">
            {t('auth.register')}
          </Link>
        </p>
      </div>
    </div>
  );
}
