/**
 * DeliveryHistory.tsx — 連携の受信履歴(直近 20 件)
 *
 * 連携カードの「受信履歴」を開いたときだけ取得する。何が届いて、どう処理されたか(紐付いた・番号が見つからない・対象外)を
 * 後から見られる。設計: docs/design/詳細設計_GitHub連携Phase2.md §7
 */
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { apiClient } from '@/shared/api/client';
import type { GitDelivery } from '@/shared/api/types';

interface DeliveryHistoryProps {
  integrationId: number;
}

export function DeliveryHistory({ integrationId }: DeliveryHistoryProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const { data: rows = [], isLoading } = useQuery<GitDelivery[]>({
    queryKey: ['integration-deliveries', integrationId],
    queryFn: async () => {
      const res = await apiClient.get(`/integrations/${integrationId}/deliveries/`);
      return res.data;
    },
    enabled: open,
    // 開いている間は、新しい受信が見えるよう取り直す
    refetchInterval: open ? 30_000 : false,
  });

  return (
    <details
      className="integration-card__history"
      onToggle={(e) => setOpen((e.currentTarget as HTMLDetailsElement).open)}
    >
      <summary>{t('integration.github.history.title')}</summary>
      {open && isLoading && <div>{t('integration.github.diagnostics.loading')}</div>}
      {open && !isLoading && rows.length === 0 && (
        <div className="integration-card__history-empty">{t('integration.github.history.empty')}</div>
      )}
      {rows.length > 0 && (
        <ul className="integration-card__history-list">
          {rows.map((r, i) => (
            <li key={`${r.receivedAt}-${i}`} className={`integration-card__history-item integration-card__history-item--${r.result}`}>
              <span className="integration-card__history-time">{new Date(r.receivedAt).toLocaleString()}</span>
              <span className="integration-card__history-event">{r.event}</span>
              <span className="integration-card__history-result">
                {t(`integration.github.diagnostics.${r.result}`, { defaultValue: r.result })}
              </span>
              {r.note && <span className="integration-card__history-note">{r.note}</span>}
            </li>
          ))}
        </ul>
      )}
    </details>
  );
}
