/**
 * LoginForm.tsx — ログインフォーム
 *
 * メール + パスワードでJWT認証。
 * Zodバリデーション + 入力値保持 + エラー表示。
 */

import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate } from 'react-router-dom';
import { z } from 'zod';
import { useAuthStore } from '@/shared/stores/authStore';
import './LoginForm.css';

const loginSchema = z.object({
  email: z.string().email('Invalid email address'),
  password: z.string().min(1, 'Password is required'),
});

export function LoginForm() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const login = useAuthStore((s) => s.login);

  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError('');

    // フロント側バリデーション（Layer 1）
    const result = loginSchema.safeParse({ email, password });
    if (!result.success) {
      setError(result.error.issues[0]?.message ?? 'Validation error');
      return;
    }

    setIsLoading(true);
    try {
      await login(email, password);
      navigate('/dashboard');
    } catch {
      setError('Invalid email or password');
    } finally {
      setIsLoading(false);
    }
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
        <form className="login__form" onSubmit={(e) => { void handleSubmit(e); }}>
          {error && (
            <div className="login__error" data-testid="login-error" role="alert">
              {error}
            </div>
          )}

          <div className="login__field">
            <label htmlFor="email" className="login__label">
              {t('auth.email')}
            </label>
            <input
              id="email"
              type="email"
              className="login__input"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="name@example.com"
              autoComplete="email"
              autoFocus
              data-testid="login-email"
            />
          </div>

          <div className="login__field">
            <div className="login__label-row">
              <label htmlFor="password" className="login__label">
                {t('auth.password')}
              </label>
              <Link to="/forgot-password" className="login__forgot">
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

        {/* サインアップリンク */}
        <p className="login__signup">
          Don't have an account?{' '}
          <Link to="/register" data-testid="register-link">
            {t('auth.register')}
          </Link>
        </p>
      </div>
    </div>
  );
}
