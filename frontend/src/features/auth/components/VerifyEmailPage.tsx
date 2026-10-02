/**
 * VerifyEmailPage.tsx — 社内ドメインの自己登録のメール確認(アクセス制御の再設計 A-3・A-4)
 *
 * /verify-email?token=...。開いたら確認を送り、結果を表示する(確認が済むとログインできる)。
 */
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useSearchParams } from 'react-router-dom';
import type { AxiosError } from 'axios';
import { apiClient } from '@/shared/api/client';
import { LanguageToggle } from '@/shared/components/ui/LanguageToggle';
import './LoginForm.css';

type State = { kind: 'loading' } | { kind: 'ok' } | { kind: 'error'; message: string };

export function VerifyEmailPage() {
  const { t } = useTranslation();
  const [searchParams] = useSearchParams();
  const token = searchParams.get('token') ?? '';
  const [state, setState] = useState<State>({ kind: 'loading' });
  // 確認のトークンは 1 回限り。開発時の StrictMode で効果が 2 回走っても、送るのは 1 回だけにする
  const sentFor = useRef<string | null>(null);

  useEffect(() => {
    if (sentFor.current === token) return;
    sentFor.current = token;
    void (async () => {
      try {
        await apiClient.post('/auth/verify-email/', { token });
        setState({ kind: 'ok' });
      } catch (err) {
        const detail = (err as AxiosError<{ detail?: string }>)?.response?.data?.detail;
        setState({ kind: 'error', message: detail ?? t('invite.verifyFailed') });
      }
    })();
  }, [token, t]);

  return (
    <div className="login" data-testid="verify-email-page">
      <LanguageToggle className="lang-toggle--floating" />
      <div className="login__card">
        <div className="login__header">
          <h1 className="login__logo">SENN</h1>
          <p className="login__tagline">{t('invite.verifyTitle')}</p>
        </div>
        {state.kind === 'loading' && <p className="login__hint">{t('common.loading')}</p>}
        {state.kind === 'ok' && (
          <p className="login__hint" data-testid="verify-email-ok">
            {t('invite.verified')}
          </p>
        )}
        {state.kind === 'error' && (
          <div className="login__error" role="alert" data-testid="verify-email-error">
            {state.message}
          </div>
        )}
        <p className="login__signup">
          <Link to="/login">{t('auth.backToLogin')}</Link>
        </p>
      </div>
    </div>
  );
}
