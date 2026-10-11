/**
 * ForbiddenNotice.tsx — 連携の設定を変更できない人(403)に、理由を伝える表示
 *
 * 権限が無いのに「読み込み中…」のまま止まったり、「未設定」と取り違えられたりしないようにする。
 */
import { useTranslation } from 'react-i18next';

export function isForbiddenError(err: unknown): boolean {
  return (err as { response?: { status?: number } } | null)?.response?.status === 403;
}

export function ForbiddenNotice() {
  const { t } = useTranslation();
  return (
    <div className="integration-empty" role="status">
      <div className="integration-empty__icon">🔒</div>
      <p>{t('integration.forbidden.title')}</p>
      <p className="integration-empty__hint">{t('integration.forbidden.hint')}</p>
    </div>
  );
}
