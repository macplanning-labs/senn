/**
 * TicketListPage.tsx — チケット一覧 + 右ペイン詳細
 *
 * 左: チケットテーブル（TicketTable）
 * 右: チケット詳細パネル（TicketDetailPanel）— URLに :ticketId があれば表示
 *
 * URLパターン:
 *   /p/:projectKey/tickets       → 一覧のみ
 *   /p/:projectKey/tickets/:id   → 一覧 + 右ペイン
 */

import { useParams, useNavigate } from 'react-router-dom';
import { TicketTable } from './TicketTable';
import { TicketDetailPanel } from './TicketDetailPanel';
import './TicketListPage.css';

export function TicketListPage() {
  const { projectKey, ticketId } = useParams<{ projectKey: string; ticketId: string }>();
  const navigate = useNavigate();

  const handleClosePanel = () => {
    if (projectKey) {
      navigate(`/p/${projectKey}/tickets`);
    }
  };

  return (
    <div className={`ticket-list-page ${ticketId ? 'ticket-list-page--with-panel' : ''}`} data-testid="ticket-list-page">
      <div className="ticket-list-page__table">
        <TicketTable />
      </div>
      {ticketId && (
        <div className="ticket-list-page__panel">
          <TicketDetailPanel ticketId={ticketId} onClose={handleClosePanel} />
        </div>
      )}
    </div>
  );
}
