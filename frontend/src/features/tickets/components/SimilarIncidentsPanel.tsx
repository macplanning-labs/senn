/**
 * SimilarIncidentsPanel.tsx — 新規バグ起票画面の「過去の類似障害」
 *
 * 起票の途中で、似た過去の障害(原因・対策つき)を見せ、同じ調査を繰り返さずに済むようにする。
 * 表示だけの付加情報。1 件も無ければ何も出さない。本文はすべて文字として表示する(HTML として解釈しない)。
 */

import { useTranslation } from 'react-i18next';
import type { SimilarIncident } from '@/shared/api/types';
import { buildTicketDetailPath } from '../utils/ticketNavigation';

interface SimilarIncidentsPanelProps {
  projectPrefix: string;
  incidents: SimilarIncident[];
  /** 検索が実行できたか(0 件でも true)。false のときは何も出さない */
  searched: boolean;
  /** 説明が空か。空のときは「説明も書くと見つかりやすい」と案内する */
  descriptionEmpty: boolean;
}

const panelStyle: React.CSSProperties = {
  border: '1px solid var(--color-border-default)',
  borderRadius: 'var(--radius-md)',
  padding: 'var(--space-3, 12px)',
  marginTop: 'var(--space-2, 8px)',
  fontSize: 'var(--font-size-sm)',
};

const noteStyle: React.CSSProperties = {
  margin: '2px 0 0',
  color: 'var(--color-text-secondary)',
};

export function SimilarIncidentsPanel({
  projectPrefix,
  incidents,
  searched,
  descriptionEmpty,
}: SimilarIncidentsPanelProps) {
  const { t } = useTranslation();
  if (!searched) return null;
  if (incidents.length === 0) {
    // 見つからなかったとき。説明が空なら、書くと見つかりやすくなることだけを短く案内する
    return descriptionEmpty ? (
      <p style={{ ...noteStyle, marginTop: 'var(--space-2, 8px)' }} data-testid="similar-incidents-hint">
        {t('similarIncidents.writeDescriptionHint')}
      </p>
    ) : null;
  }

  return (
    <section style={panelStyle} aria-label={t('similarIncidents.title')} data-testid="similar-incidents">
      <h4 style={{ margin: 0, fontSize: 'var(--font-size-sm)' }}>🔎 {t('similarIncidents.title')}</h4>
      <p style={noteStyle}>{t('similarIncidents.hint')}</p>
      {descriptionEmpty && (
        <p style={noteStyle} data-testid="similar-incidents-hint">
          {t('similarIncidents.writeDescriptionHint')}
        </p>
      )}
      <ul style={{ listStyle: 'none', margin: 'var(--space-2, 8px) 0 0', padding: 0 }}>
        {incidents.map((inc) => (
          <li
            key={inc.ticketKey}
            style={{ padding: 'var(--space-2, 8px) 0', borderTop: '1px solid var(--color-border-default)' }}
            data-testid="similar-incident"
          >
            <a
              href={buildTicketDetailPath(projectPrefix, inc.ticketKey)}
              target="_blank"
              rel="noopener noreferrer"
              style={{ color: 'var(--color-accent-primary)' }}
            >
              {inc.ticketKey}
            </a>{' '}
            <span>{inc.title}</span>
            {inc.reasons.length > 0 && <p style={noteStyle}>{inc.reasons.join(' / ')}</p>}
            {(inc.symptom || inc.cause || inc.solution) && (
              <details style={{ marginTop: 'var(--space-1, 4px)' }} data-testid="similar-incident-detail">
                <summary style={{ cursor: 'pointer', color: 'var(--color-text-secondary)' }}>
                  {t('similarIncidents.showDetail')}
                </summary>
              {inc.symptom && (
                <p style={noteStyle}>
                  {t('similarIncidents.symptom')}: {inc.symptom}
                </p>
              )}
              {inc.cause && (
                <p style={noteStyle}>
                  {t('similarIncidents.cause')}: {inc.cause}
                </p>
              )}
              {inc.solution && (
                <p style={noteStyle}>
                  {t('similarIncidents.solution')}: {inc.solution}
                </p>
              )}
              </details>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

/**
 * プロジェクトが決まっていない(チームの画面から作る場合など)と、類似障害は探せない。
 * 黙って何も出さず、「プロジェクトを選ぶと探せる」ことだけを案内する。
 */
export function SimilarIncidentsNeedProjectHint() {
  const { t } = useTranslation();
  return (
    <p style={{ ...noteStyle, marginTop: 'var(--space-2, 8px)' }} data-testid="similar-incidents-need-project">
      {t('similarIncidents.selectProjectHint')}
    </p>
  );
}
