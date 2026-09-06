/**
 * RegisterForm.tsx — 新規登録フォーム
 *
 * ユーザー名 + メール + パスワードでアカウントを作成し、
 * 発行されたJWTでそのままログイン状態にしてダッシュボードへ遷移する。
 * デザイン/スタイルは LoginForm と共通の CSS(LoginForm.css) を再利用。
 */

import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate } from 'react-router-dom';
import { z } from 'zod';
import type { AxiosError } from 'axios';
import { useAuthStore } from '@/shared/stores/authStore';
import { apiClient } from '@/shared/api/client';
import './LoginForm.css';

const registerSchema = z.object({
  username: z.string().min(1, 'Username is required'),
  email: z.string().min(1, 'Email is required').email('Enter a valid email address'),
  password: z.string().min(8, 'Password must be at least 8 characters'),
});

interface RegisterResponse {
  user: { id: number; username: string; email: string };
  tokens: { access: string; refresh: string };
}

export function RegisterForm() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  // ログイン済み状態への反映は authStore の loginWithPasskey を流用する。
  // (パスキー専用ではなく「発行済みトークンで認証完了させる」汎用処理のため)
  const completeAuth = useAuthStore((s) => s.loginWithPasskey);

  const [username, setUsername] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError('');

    const result = registerSchema.safeParse({ username, email, password });
    if (!result.success) {
      setError(result.error.issues[0]?.message ?? 'Validation error');
      return;
    }

    setIsLoading(true);
    try {
      const { data } = await apiClient.post<RegisterResponse>('/auth/register/', {
        username,
        email,
        password,
      });
      await completeAuth(data.tokens.access, data.tokens.refresh);
      navigate('/dashboard');
    } catch (err) {
      const detail = (err as AxiosError<{ detail?: string }>)?.response?.data?.detail;
      setError(detail || t('auth.registerError', 'Failed to create account'));
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <div className="login" data-testid="register-page">
      <div className="login__card">
        <div className="login__header">
          <h1 className="login__logo">WIP</h1>
          <p className="login__tagline">{t('app.tagline')}</p>
        </div>

        <form className="login__form" onSubmit={(e) => { void handleSubmit(e); }} data-testid="register-form">
          {error && (
            <div className="login__error" data-testid="register-error" role="alert">
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
              autoComplete="username"
              autoFocus
              data-testid="register-username"
            />
          </div>

          <div className="login__field">
            <label htmlFor="email" className="login__label">
              {t('auth.email')}
            </label>
            <input
              id="email"
              type="email"
              name="email"
              className="login__input"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              autoComplete="email"
              data-testid="register-email"
            />
          </div>

          <div className="login__field">
            <label htmlFor="password" className="login__label">
              {t('auth.password')}
            </label>
            <input
              id="password"
              type="password"
              className="login__input"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="new-password"
              data-testid="register-password"
            />
            <p className="login__hint">
              {t('auth.passwordHint', 'At least 8 characters')}
            </p>
          </div>

          <button
            type="submit"
            className="login__submit"
            disabled={isLoading}
            data-testid="register-submit"
          >
            {isLoading ? t('common.loading') : t('auth.register')}
          </button>
        </form>

        <p className="login__signup">
          {t('auth.haveAccount', 'Already have an account?')}{' '}
          <Link to="/login" data-testid="login-link">
            {t('auth.login')}
          </Link>
        </p>
      </div>
    </div>
  );
}
