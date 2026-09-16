/**
 * TicketListPage.tsx — チケット一覧 + 右ペイン詳細
 *
 * 左: チケットテーブル（TicketTable）
 * 右: チケット詳細パネル（TicketDetailPanel）— URLに :ticketId があれば表示
 *
 * URLパターン:
 *   /project/:projectKey/tickets       → 一覧のみ
 *   /project/:projectKey/tickets/:id   → 一覧 + 右ペイン
 *   /team/:teamSlug/tickets         → Team一覧のみ
 *   /team/:teamSlug/tickets/:id     → Team一覧 + 右ペイン
 */

import { useParams, useNavigate } from 'react-router-dom';
import { TicketTable } from './TicketTable';
import { TicketDetailPanel } from './TicketDetailPanel';
import { usePanelResize } from '@/shared/hooks/usePanelResize';
import './TicketListPage.css';

export function TicketListPage() {
  const { projectKey, teamSlug, ticketId } = useParams<{ projectKey?: string; teamSlug?: string; ticketId?: string }>();
  const navigate = useNavigate();
  const { width: panelWidth, onResizeStart, isResizing } = usePanelResize('ticket-detail', 380);

  const handleClosePanel = () => {
    if (projectKey) {
      navigate(`/project/${projectKey}/tickets`);
    } else if (teamSlug) {
      navigate(`/team/${teamSlug}/tickets`);
    }
  };

  return (
    <div
      className={`ticket-list-page ${ticketId ? 'ticket-list-page--with-panel' : ''} ${isResizing ? 'ticket-list-page--resizing' : ''}`}
      data-testid="ticket-list-page"
      style={ticketId ? { gridTemplateColumns: `1fr ${panelWidth}px` } : undefined}
    >
      <div className="ticket-list-page__table">
        <TicketTable />
      </div>
      {ticketId && (
        <div className="ticket-list-page__panel">
          <div
            className="ticket-list-page__resize-handle"
            onMouseDown={onResizeStart}
            data-testid="panel-resize-handle"
          />
          <TicketDetailPanel ticketId={ticketId} onClose={handleClosePanel} />
        </div>
      )}
    </div>
  );
}
