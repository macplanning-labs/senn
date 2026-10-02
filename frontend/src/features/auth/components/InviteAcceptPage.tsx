/**
 * InviteAcceptPage.tsx — 招待の受諾とアカウントの作成(アクセス制御の再設計 A-4)
 *
 * /invite/:token。招待の内容(宛先のメールアドレスの一部・チーム・プロジェクト)を見せ、
 * ユーザー名・表示名・パスワードを決めてアカウントを作る。作れたら、そのままログインする。
 */
import { useEffect, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate, useParams } from 'react-router-dom';
import type { AxiosError } from 'axios';
import { apiClient } from '@/shared/api/client';
import { useAuthStore } from '@/shared/stores/authStore';
import { LanguageToggle } from '@/shared/components/ui/LanguageToggle';
import './LoginForm.css';

interface InvitationPreview {
  emailMasked: string;
  role: 'full_member' | 'guest';
  teamName: string | null;
  projectName: string | null;
  expiresAt: string;
}

type Loaded = { kind: 'loading' } | { kind: 'invalid'; message: string } | { kind: 'ok'; invitation: InvitationPreview };

function errorDetail(err: unknown): string | undefined {
  return (err as AxiosError<{ detail?: string }>)?.response?.data?.detail;
}

export function InviteAcceptPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { token = '' } = useParams<{ token: string }>();
  // 発行済みのトークンでログイン状態にする(登録の画面と同じ処理)
  const completeAuth = useAuthStore((s) => s.loginWithPasskey);

  const [loaded, setLoaded] = useState<Loaded>({ kind: 'loading' });
  const [username, setUsername] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const { data } = await apiClient.get<InvitationPreview>(`/invitations/${token}/`);
        if (!cancelled) setLoaded({ kind: 'ok', invitation: data });
      } catch (err) {
        if (!cancelled) setLoaded({ kind: 'invalid', message: errorDetail(err) ?? t('invite.invalid') });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [token, t]);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError('');
    if (!username.trim()) {
      setError(t('invite.usernameRequired'));
      return;
    }
    if (password.length < 8) {
      setError(t('settings.passwordTooShort'));
      return;
    }
    if (password !== confirm) {
      setError(t('auth.resetPasswordMismatch'));
      return;
    }
    setIsLoading(true);
    try {
      const { data } = await apiClient.post<{ tokens: { access: string; refresh: string } }>(
        `/invitations/${token}/accept/`,
        { username: username.trim(), password, displayName: displayName.trim() },
      );
      await completeAuth(data.tokens.access, data.tokens.refresh);
      navigate('/my-issues', { replace: true });
    } catch (err) {
      setError(errorDetail(err) ?? t('invite.acceptFailed'));
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <div className="login" data-testid="invite-accept-page">
      <LanguageToggle className="lang-toggle--floating" />
      <div className="login__card">
        <div className="login__header">
          <h1 className="login__logo">SENN</h1>
          <p className="login__tagline">{t('invite.acceptTitle')}</p>
        </div>

        {loaded.kind === 'loading' && <p className="login__hint">{t('common.loading')}</p>}

        {loaded.kind === 'invalid' && (
          <>
            <div className="login__error" role="alert" data-testid="invite-invalid">
              {loaded.message}
            </div>
            <p className="login__signup">
              <Link to="/login">{t('auth.backToLogin')}</Link>
            </p>
          </>
        )}

        {loaded.kind === 'ok' && (
          <form className="login__form" onSubmit={(e) => { void handleSubmit(e); }}>
            <div className="login__hint" data-testid="invite-summary">
              <p>{t('invite.to', { email: loaded.invitation.emailMasked })}</p>
              {loaded.invitation.teamName && (
                <p>
                  {loaded.invitation.projectName
                    ? t('invite.intoProject', {
                        team: loaded.invitation.teamName,
                        project: loaded.invitation.projectName,
                      })
                    : t('invite.intoTeam', { team: loaded.invitation.teamName })}
                </p>
              )}
              <p>{loaded.invitation.role === 'guest' ? t('invite.asGuest') : t('invite.asFullMember')}</p>
            </div>

            {error && (
              <div className="login__error" role="alert" data-testid="invite-error">
                {error}
              </div>
            )}

            <div className="login__field">
              <label htmlFor="invite-username" className="login__label">
                {t('auth.username', 'Username')}
              </label>
              <input
                id="invite-username"
                className="login__input"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoComplete="username"
                autoFocus
                data-testid="invite-username"
              />
            </div>
            <div className="login__field">
              <label htmlFor="invite-display-name" className="login__label">
                {t('invite.displayName')}
              </label>
              <input
                id="invite-display-name"
                className="login__input"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                autoComplete="name"
                data-testid="invite-display-name"
              />
            </div>
            <div className="login__field">
              <label htmlFor="invite-password" className="login__label">
                {t('auth.password')}
              </label>
              <input
                id="invite-password"
                type="password"
                className="login__input"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="new-password"
                data-testid="invite-password"
              />
              <p className="login__hint">{t('auth.passwordHint', 'At least 8 characters')}</p>
            </div>
            <div className="login__field">
              <label htmlFor="invite-confirm" className="login__label">
                {t('auth.confirmPassword')}
              </label>
              <input
                id="invite-confirm"
                type="password"
                className="login__input"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                autoComplete="new-password"
                data-testid="invite-confirm"
              />
            </div>
            <button type="submit" className="login__submit" disabled={isLoading} data-testid="invite-accept-submit">
              {isLoading ? t('common.loading') : t('invite.createAccount')}
            </button>
          </form>
        )}
      </div>
    </div>
  );
}
