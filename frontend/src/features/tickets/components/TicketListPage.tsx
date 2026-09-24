/**
 * TicketListPage.tsx — チケット一覧（リストのみ）
 *
 * チケットテーブル（TicketTable）を表示する。
 * :ticketId 付き URL は App routes 経由で TicketDetailPage へ遷移する。
 *
 * URLパターン:
 *   /project/:projectKey/tickets       → 一覧のみ
 *   /project/:projectKey/tickets/:id   → TicketDetailPage
 *   /team/:teamSlug/tickets            → Team一覧のみ
 *   /team/:teamSlug/tickets/:id        → TicketDetailPage
 */

import { TicketTable } from './TicketTable';
import './TicketListPage.css';

export function TicketListPage() {
  return (
    <div className="ticket-list-page" data-testid="ticket-list-page">
      <div className="ticket-list-page__table">
        <TicketTable />
      </div>
    </div>
  );
}
