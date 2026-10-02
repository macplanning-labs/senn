/**
 * ResetPasswordPage.tsx — 新パスワード設定（メールリンクから遷移）
 */

import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { apiClient } from '@/shared/api/client';
import './LoginForm.css';
import { LanguageToggle } from '@/shared/components/ui/LanguageToggle';

export function ResetPasswordPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const token = searchParams.get('token') ?? '';

  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [error, setError] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError('');

    if (!token) {
      setError(t('auth.resetTokenMissing'));
      return;
    }

    if (newPassword.length < 8) {
      setError(t('settings.passwordTooShort'));
      return;
    }

    if (newPassword !== confirmPassword) {
      setError(t('auth.resetPasswordMismatch'));
      return;
    }

    setIsLoading(true);
    try {
      await apiClient.post('/auth/password-reset/confirm/', {
        token,
        new_password: newPassword,
      });
      navigate('/login', {
        replace: true,
        state: { resetSuccess: true },
      });
    } catch (err: unknown) {
      const axiosErr = err as { response?: { data?: { detail?: string } } };
      setError(
        axiosErr.response?.data?.detail
          || t('auth.resetConfirmFailed'),
      );
    } finally {
      setIsLoading(false);
    }
  }

  if (!token) {
    return (
      <div className="login" data-testid="reset-password-page">
        <LanguageToggle className="lang-toggle--floating" />
        <div className="login__card">
          <div className="login__header">
            <h1 className="login__logo">SENN</h1>
          </div>
          <div className="login__error" role="alert">
            {t('auth.resetTokenMissing')}
          </div>
          <p className="login__signup">
            <Link to="/forgot-password">{t('auth.forgotPassword')}</Link>
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="login" data-testid="reset-password-page">
      <div className="login__card">
        <div className="login__header">
          <h1 className="login__logo">SENN</h1>
          <p className="login__tagline">
            {t('auth.resetPasswordTitle')}
          </p>
        </div>

        <form className="login__form" onSubmit={(e) => { void handleSubmit(e); }}>
          {error && (
            <div className="login__error" role="alert">
              {error}
            </div>
          )}

          <div className="login__field">
            <label htmlFor="new-password" className="login__label">
              {t('auth.newPassword')}
            </label>
            <input
              id="new-password"
              type="password"
              className="login__input"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              autoComplete="new-password"
              autoFocus
              data-testid="reset-new-password-input"
            />
          </div>

          <div className="login__field">
            <label htmlFor="confirm-password" className="login__label">
              {t('auth.confirmPassword')}
            </label>
            <input
              id="confirm-password"
              type="password"
              className="login__input"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              autoComplete="new-password"
              data-testid="reset-confirm-password-input"
            />
          </div>

          <button
            type="submit"
            className="login__submit"
            disabled={isLoading}
            data-testid="reset-password-submit"
          >
            {isLoading ? t('common.loading') : t('auth.updatePassword')}
          </button>

          <p className="login__signup">
            <Link to="/login">{t('auth.backToLogin')}</Link>
          </p>
        </form>
      </div>
    </div>
  );
}
