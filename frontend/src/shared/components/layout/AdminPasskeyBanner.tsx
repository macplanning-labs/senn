/**
 * AdminPasskeyBanner.tsx — 管理者のパスキー登録の案内(ログイン後の画面の上部)
 *
 * adminPasskey.mustRegister が true の間だけ出す。登録するまで閉じるボタンは付けない。
 * 強制の開始日(enforceAfter)があれば、期限を末尾に添える。
 */
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '@/shared/stores/authStore';
import './AdminPasskeyBanner.css';

/** 期限の表示。日付の並びは言語ごとに i18n の adminPasskey.dateFormat で決める。解釈できなければ null */
function formatDeadline(value: string | null, t: (key: string, opts: Record<string, unknown>) => string): string | null {
  if (!value) return null;
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return null;
  return t('adminPasskey.dateFormat', {
    year: d.getFullYear(),
    month: d.getMonth() + 1,
    day: d.getDate(),
    monthName: d.toLocaleDateString('en-US', { month: 'short' }),
  });
}

export function AdminPasskeyBanner() {
  const { t } = useTranslation();
  const adminPasskey = useAuthStore((s) => s.user?.adminPasskey);

  if (adminPasskey?.mustRegister !== true) return null;

  const deadline = formatDeadline(adminPasskey.enforceAfter, t);

  return (
    <div className="admin-passkey-banner" role="status" data-testid="admin-passkey-banner">
      <p className="admin-passkey-banner__text">
        <span aria-hidden="true">🔑 </span>
        {t('adminPasskey.bannerMessage')}
        {deadline && <> {t('adminPasskey.bannerDeadline', { date: deadline })}</>}
      </p>
      <Link className="admin-passkey-banner__link" to="/settings?tab=security" data-testid="admin-passkey-banner-link">
        {t('adminPasskey.bannerLink')}
      </Link>
    </div>
  );
}
