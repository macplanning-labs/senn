/**
 * NotFoundPage.tsx — 見つからない・閲覧できないページ(アクセス制御の再設計 G-3)
 *
 * サーバーは、見えない物を「存在しない」と同じ 404 で返す(存在を推測させない)。画面も同じ文言にする。
 * ログインしていなければ、ログインの画面へ送る(今までの動作)。
 */
import { Link, Navigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '@/shared/stores/authStore';

export function NotFoundPage() {
  const { t } = useTranslation();
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }
  return (
    <div
      data-testid="not-found-page"
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: '1rem',
        minHeight: '60vh',
        color: 'var(--color-text-secondary)',
      }}
    >
      <p style={{ fontSize: '1rem', margin: 0 }}>{t('teamAccess.notFoundPage')}</p>
      <Link to="/" style={{ color: 'var(--color-accent-primary)' }}>
        {t('teamAccess.backHome')}
      </Link>
    </div>
  );
}
