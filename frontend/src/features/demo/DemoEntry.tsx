/**
 * DemoEntry — お客様向けビルド専用のデモ入口（/demo）
 *
 * VITE_APP_CHANNEL=customer のときだけ有効。業務用 /login とは分離。
 * 正本: docs/方針_ビルドチャネル_customerとinternal.md
 */

import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '@/shared/stores/authStore';

export function DemoEntry() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const enterDemoMode = useAuthStore((s) => s.enterDemoMode);
  const [error, setError] = useState('');

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        await enterDemoMode();
        if (!cancelled) {
          navigate('/my-issues', { replace: true });
        }
      } catch {
        if (!cancelled) {
          setError(t('demo.error', 'Failed to enter demo mode'));
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [enterDemoMode, navigate, t]);

  return (
    <div className="login" data-testid="demo-entry">
      <div className="login__card">
        <div className="login__header">
          <h1 className="login__logo">SENN</h1>
          <p className="login__tagline">
            {error || t('demo.entering', 'Starting demo…')}
          </p>
        </div>
        {error && (
          <p className="login__signup">
            <a href="/login">{t('auth.backToLogin', 'Back to login')}</a>
          </p>
        )}
      </div>
    </div>
  );
}
