/**
 * ForgotPasswordPage.tsx — パスワードリセット要求
 */

import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import type { AxiosError } from 'axios';
import { apiClient } from '@/shared/api/client';
import './LoginForm.css';
import { LanguageToggle } from '@/shared/components/ui/LanguageToggle';

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
      setError(t('auth.resetIdentifierRequired'));
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
          || t('auth.resetRequestSent'),
      );
      if (res.data.reset_url) {
        setResetUrl(res.data.reset_url);
      }
    } catch (err) {
      const axiosErr = err as AxiosError<{ detail?: string; message?: string }>;
      const status = axiosErr.response?.status;
      const detail =
        axiosErr.response?.data?.detail
        || axiosErr.response?.data?.message
        || (typeof axiosErr.response?.data === 'string' ? axiosErr.response.data : undefined);
      const fallback = t('auth.resetRequestFailed');
      if (detail) {
        setError(status ? `${detail} (HTTP ${status})` : detail);
      } else if (status) {
        setError(`${fallback} (HTTP ${status})`);
      } else if (axiosErr.message) {
        setError(`${fallback} (${axiosErr.message})`);
      } else {
        setError(fallback);
      }
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <div className="login" data-testid="forgot-password-page">
      <LanguageToggle className="lang-toggle--floating" />
      <div className="login__card">
        <div className="login__header">
          <h1 className="login__logo">SENN</h1>
          <p className="login__tagline">
            {t('auth.forgotPasswordTitle')}
          </p>
        </div>

        {successMessage ? (
          <div className="login__success" role="status">
            <p>{successMessage}</p>
            {resetUrl && (
              <p className="login__hint" style={{ marginTop: 'var(--space-3)' }}>
                <a href={resetUrl} className="login__forgot" data-testid="reset-url-link">
                  {t('auth.openResetLink')}
                </a>
              </p>
            )}
            <Link to="/login" className="login__back-link">
              {t('auth.backToLogin')}
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
              {t('auth.forgotPasswordHint')}
            </p>

            <div className="login__field">
              <label htmlFor="reset-identifier" className="login__label">
                {t('auth.resetIdentifier')}
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
              {isLoading ? t('common.loading') : t('auth.sendResetLink')}
            </button>

            <p className="login__signup">
              <Link to="/login">{t('auth.backToLogin')}</Link>
            </p>
          </form>
        )}
      </div>
    </div>
  );
}
