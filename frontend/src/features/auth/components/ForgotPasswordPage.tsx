/**
 * ForgotPasswordPage.tsx — パスワードリセット要求
 */

import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import './LoginForm.css';

export function ForgotPasswordPage() {
  const { t } = useTranslation();
  const [identifier, setIdentifier] = useState('');
  const [error, setError] = useState('');
  const [successMessage, setSuccessMessage] = useState('');
  const [resetUrl, setResetUrl] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError('');
    setSuccessMessage('');
    setResetUrl('');

    const trimmed = identifier.trim();
    if (!trimmed) {
      setError(t('auth.resetIdentifierRequired', 'メールアドレスまたはユーザー名を入力してください'));
      return;
    }

    setIsLoading(true);
    try {
      const res = await apiClient.post<{ message: string; reset_url?: string }>(
        '/auth/password-reset/request/',
        { identifier: trimmed },
      );
      setSuccessMessage(
        res.data.message
          || t('auth.resetRequestSent', '登録されている場合、パスワードリセット用のメールを送信しました。'),
      );
      if (res.data.reset_url) {
        setResetUrl(res.data.reset_url);
      }
    } catch {
      setError(t('auth.resetRequestFailed', 'リクエストに失敗しました。しばらくしてから再度お試しください。'));
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <div className="login" data-testid="forgot-password-page">
      <div className="login__card">
        <div className="login__header">
          <h1 className="login__logo">WIP</h1>
          <p className="login__tagline">
            {t('auth.forgotPasswordTitle', 'パスワードをリセット')}
          </p>
        </div>

        {successMessage ? (
          <div className="login__success" role="status">
            <p>{successMessage}</p>
            {resetUrl && (
              <p className="login__hint" style={{ marginTop: 'var(--space-3)' }}>
                <a href={resetUrl} className="login__forgot" data-testid="reset-url-link">
                  {t('auth.openResetLink', 'パスワード再設定リンクを開く')}
                </a>
              </p>
            )}
            <Link to="/login" className="login__back-link">
              {t('auth.backToLogin', 'ログイン画面に戻る')}
            </Link>
          </div>
        ) : (
          <form className="login__form" onSubmit={(e) => { void handleSubmit(e); }}>
            {error && (
              <div className="login__error" role="alert">
                {error}
              </div>
            )}

            <p className="login__hint">
              {t(
                'auth.forgotPasswordHint',
                '登録済みのメールアドレスまたはユーザー名を入力してください。リセット用リンクをお送りします。',
              )}
            </p>

            <div className="login__field">
              <label htmlFor="reset-identifier" className="login__label">
                {t('auth.resetIdentifier', 'メールアドレスまたはユーザー名')}
              </label>
              <input
                id="reset-identifier"
                type="text"
                className="login__input"
                value={identifier}
                onChange={(e) => setIdentifier(e.target.value)}
                autoComplete="username"
                autoFocus
                data-testid="reset-identifier-input"
              />
            </div>

            <button
              type="submit"
              className="login__submit"
              disabled={isLoading}
              data-testid="reset-request-submit"
            >
              {isLoading ? t('common.loading') : t('auth.sendResetLink', 'リセットリンクを送信')}
            </button>

            <p className="login__signup">
              <Link to="/login">{t('auth.backToLogin', 'ログイン画面に戻る')}</Link>
            </p>
          </form>
        )}
      </div>
    </div>
  );
}
