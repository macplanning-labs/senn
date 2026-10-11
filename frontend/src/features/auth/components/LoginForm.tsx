/**
 * LoginForm.tsx — ログインフォーム
 *
 * ユーザー名 + パスワードでJWT認証。
 * Zodバリデーション + 入力値保持 + エラー表示。
 * SSO（Cloudflare Access）対応。
 */

import { useState, useEffect, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { z } from 'zod';
import { startAuthentication } from '@simplewebauthn/browser';
import { useAuthStore, type SsoAccount } from '@/shared/stores/authStore';
import { apiClient } from '@/shared/api/client';
import { rateLimitMessage } from '../rateLimitMessage';
import './LoginForm.css';
import { LanguageToggle } from '@/shared/components/ui/LanguageToggle';

const loginSchema = z.object({
  username: z.string().min(1, 'Username is required'),
  password: z.string().min(1, 'Password is required'),
});

export function LoginForm() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const resetSuccess = (location.state as { resetSuccess?: boolean } | null)?.resetSuccess;
  const login = useAuthStore((s) => s.login);
  const verifyMfa = useAuthStore((s) => s.verifyMfa);
  const loginWithPasskey = useAuthStore((s) => s.loginWithPasskey);
  const loginWithSso = useAuthStore((s) => s.loginWithSso);

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  // MFA（TOTP）2段階目
  const [mfaToken, setMfaToken] = useState<string | null>(null);
  const [totpCode, setTotpCode] = useState('');

  // SSO 関連のstate
  const [ssoAvailable, setSsoAvailable] = useState<boolean | undefined>(undefined);
  const [ssoAccounts, setSsoAccounts] = useState<SsoAccount[] | null>(null);
  const [ssoAutoCheckInProgress, setSsoAutoCheckInProgress] = useState(true);

  // SSO 自動試行（マウント時に1度だけ実行）
  useEffect(() => {
    const attemptSsoAuto = async () => {
      const skipAuto = sessionStorage.getItem('senn_sso_skip_auto') === '1';
      if (skipAuto) {
        setSsoAutoCheckInProgress(false);
        return;
      }

      try {
        const result = await loginWithSso();
        if (result.status === 'loggedIn') {
          navigate('/my-issues');
          return;
        } else if (result.status === 'choose') {
          setSsoAccounts(result.accounts);
          setSsoAvailable(true);
          setSsoAutoCheckInProgress(false);
          return;
        } else if (result.status === 'disabled') {
          setSsoAvailable(false);
          setSsoAutoCheckInProgress(false);
          return;
        } else if (result.status === 'failed') {
          // Silently fail on automatic attempt; keep ssoAvailable true so the button appears
          setSsoAvailable(true);
          setSsoAutoCheckInProgress(false);
          return;
        }
      } catch (err) {
        console.warn('[auth] SSO auto-attempt failed:', err);
        setSsoAvailable(true);
        setSsoAutoCheckInProgress(false);
      }
    };

    void attemptSsoAuto();
  }, [loginWithSso, navigate]);

  async function handleSsoLogin() {
    setError('');
    // Remove the skip flag so we can try SSO
    sessionStorage.removeItem('senn_sso_skip_auto');
    setSsoAutoCheckInProgress(true);
    try {
      const result = await loginWithSso();
      if (result.status === 'loggedIn') {
        navigate('/my-issues');
      } else if (result.status === 'choose') {
        setSsoAccounts(result.accounts);
      } else if (result.status === 'disabled') {
        setSsoAvailable(false);
      } else if (result.status === 'failed') {
        setError(result.detail);
      }
    } finally {
      setSsoAutoCheckInProgress(false);
    }
  }

  async function handleSsoAccountSelect(account: SsoAccount) {
    setError('');
    setSsoAutoCheckInProgress(true);
    try {
      const result = await loginWithSso(account.id);
      if (result.status === 'loggedIn') {
        navigate('/my-issues');
      } else if (result.status === 'choose') {
        setSsoAccounts(result.accounts);
      } else if (result.status === 'disabled') {
        setSsoAvailable(false);
      } else if (result.status === 'failed') {
        setError(result.detail);
      }
    } finally {
      setSsoAutoCheckInProgress(false);
    }
  }

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
      navigate('/my-issues');
    } catch (err) {
      // 429(試行ロック・回数制限)は「パスワードが違う」とは別の文言にする(DEMO-000153)
      setError(rateLimitMessage(err, t) ?? t('auth.loginError', 'Invalid username or password'));
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
      navigate('/my-issues');
    } catch (err) {
      setError(rateLimitMessage(err, t) ?? t('auth.mfaError', 'Invalid authentication code'));
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
      navigate('/my-issues');
    } catch (err: any) {
      const msg = err?.message || '';
      if (/AbortError|NotAllowedError|取消|canceled|cancelled/i.test(msg)) {
        // ユーザーが自らキャンセルした場合は、画面全体のエラー表示は
        // 出さない。静かにローディング状態を解除するのみ。
      } else {
        const detail = err?.response?.data?.detail;
        setError(detail || t('auth.passkeyAuthFailed'));
      }
    } finally {
      setIsLoading(false);
    }
  }

  // Account chooser modal for SSO
  if (ssoAccounts) {
    return (
      <div className="login" data-testid="login-page">
        <LanguageToggle className="lang-toggle--floating" />
        <div className="login__card">
          <div className="login__header">
            <h1 className="login__logo">SENN</h1>
            <p className="login__tagline">{t('auth.ssoChooseAccount', 'Which account do you want to use?')}</p>
          </div>

          <div className="login__account-chooser">
            {ssoAccounts.map((account) => (
              <button
                key={account.id}
                type="button"
                className="login__account-option"
                onClick={() => { void handleSsoAccountSelect(account); }}
                disabled={ssoAutoCheckInProgress}
              >
                <div className="login__account-name">
                  {account.displayName}
                  {account.isSystemAdmin && (
                    <>
                      {' '}
                      <span className="login__account-badge">{t('auth.ssoAdminBadge', ' (admin)')}</span>
                    </>
                  )}
                </div>
                <div className="login__account-username">{account.username}</div>
              </button>
            ))}
          </div>

          {error && (
            <div className="login__error" data-testid="login-error" role="alert">
              {error}
            </div>
          )}

          <button
            type="button"
            className="login__link-btn"
            onClick={() => { setSsoAccounts(null); setError(''); }}
            disabled={ssoAutoCheckInProgress}
          >
            {t('auth.ssoOtherMethod', 'Sign in another way')}
          </button>
        </div>
      </div>
    );
  }

  if (mfaToken) {
    return (
      <div className="login" data-testid="login-page">
        <LanguageToggle className="lang-toggle--floating" />
        <div className="login__card">
          <div className="login__header">
            <h1 className="login__logo">SENN</h1>
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

  // Show loading state during auto-SSO check
  if (ssoAutoCheckInProgress) {
    return (
      <div className="login" data-testid="login-page">
        <LanguageToggle className="lang-toggle--floating" />
        <div className="login__card">
          <div className="login__header">
            <h1 className="login__logo">SENN</h1>
            <p className="login__tagline">{t('auth.ssoChecking', 'Checking your company account…')}</p>
          </div>
          <div className="login__loading" />
        </div>
      </div>
    );
  }

  return (
    <div className="login" data-testid="login-page">
      <LanguageToggle className="lang-toggle--floating" />
      <div className="login__card">
        {/* ロゴ */}
        <div className="login__header">
          <h1 className="login__logo">SENN</h1>
          <p className="login__tagline">{t('app.tagline')}</p>
        </div>

        {/* SSO ボタン（SSO が使用可能な場合のみ表示） */}
        {ssoAvailable !== false && (
          <>
            <button
              type="button"
              className="login__sso-btn"
              onClick={() => { void handleSsoLogin(); }}
              disabled={isLoading}
              data-testid="login-sso-btn"
            >
              {t('auth.ssoLogin', 'Sign in with company account (SSO)')}
            </button>

            {/* SSO と通常ログインの区切り */}
            <div className="login__divider">
              <span>{t('auth.or', 'or')}</span>
            </div>
          </>
        )}

        {/* フォーム */}
        <form className="login__form" onSubmit={(e) => { void handleSubmit(e); }} data-testid="login-form">
          {resetSuccess && (
            <div className="login__success" role="status">
              {t('auth.resetSuccess')}
            </div>
          )}
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
