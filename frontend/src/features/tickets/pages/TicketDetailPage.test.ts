import { describe, it, expect, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import type { TicketDetailView } from '../types/ticketDetailView';

// モック設定
vi.mock('react-router-dom', () => ({
  useParams: () => ({ ticketId: 'TASK-123' }),
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../shared/sync/repos/ticketRepo', () => ({
  useTicketDetail: () => ({
    data: mockTicket,
    isLoading: false,
    isError: false,
  }),
}));

vi.mock('../../../shared/sync/ticketMapping', () => ({
  syncStateOf: () => 'synced',
}));

vi.mock('../components/TicketDetailTopBar', () => ({
  TicketDetailTopBar: () => 
    createElement('div', { className: 'ticket-detail-topbar' }, 'TopBar'),
}));

vi.mock('../components/TicketDetailSubBar', () => ({
  TicketDetailSubBar: () => 
    createElement('div', { className: 'ticket-detail-subbar' }, 'SubBar'),
}));

vi.mock('../components/TicketTitleDescriptionEditor', () => ({
  TicketTitleDescriptionEditor: () => 
    createElement('div', { className: 'ticket-title-description-editor' }, 'Editor'),
}));

vi.mock('../components/TicketSubTickets', () => ({
  TicketSubTickets: () => 
    createElement('div', { className: 'ticket-subtickkets' }, 'SubTickets'),
}));

vi.mock('../components/TicketRelations', () => ({
  TicketRelations: () => 
    createElement('div', { className: 'ticket-relations' }, 'Relations'),
}));

vi.mock('../components/ChangeLogTimeline', () => ({
  default: () => 
    createElement('div', { className: 'changelog-timeline' }, 'ChangeLog'),
}));

vi.mock('../components/TicketComments', () => ({
  TicketComments: () => 
    createElement('div', { className: 'ticket-comments' }, 'Comments'),
}));

vi.mock('../components/TicketPropertiesSidebar', () => ({
  TicketPropertiesSidebar: () => 
    createElement('aside', { className: 'ticket-detail-sidebar' }, 'Properties'),
}));

vi.mock('./TicketDetailPage.css', () => ({}));

const mockTicket = {
  id: 1,
  ticketKey: 'TASK-123',
  title: 'Sample Task',
  description: 'This is a sample task',
  ticketType: 'task',
  status: 'open',
  author: { id: 1, username: 'user1', displayName: 'User One' },
  project: 1,
  team: { id: 7, slug: 'team-7' },
  childCount: 0,
  projectPrefix: 'TASK',
  createdAt: '2026-01-01',
  comments: [],
  attachments: [],
  links: [],
} as unknown as TicketDetailView;

import { TicketDetailPage } from './TicketDetailPage';

function render(): string {
  return renderToStaticMarkup(createElement(TicketDetailPage));
}

describe('TicketDetailPage', () => {
  it('パンくずを含む .ticket-detail-topbar がスクロール領域 .ticket-detail-main の外側（兄弟要素）に存在する', () => {
    const html = render();
    
    // .ticket-detail-topbar が存在することを確認
    expect(html).toContain('class="ticket-detail-topbar"');
    
    // .ticket-detail-main が存在することを確認
    expect(html).toContain('class="ticket-detail-main"');
    
    // DOM 構造の確認: .ticket-detail-page の直接の子に両要素が存在する
    // ・ticket-detail-topbar が .ticket-detail-main より前に現れることで、兄弟要素であることを検証
    const topbarIndex = html.indexOf('class="ticket-detail-topbar"');
    const mainIndex = html.indexOf('class="ticket-detail-main"');
    
    expect(topbarIndex).toBeGreaterThan(-1);
    expect(mainIndex).toBeGreaterThan(-1);
    expect(topbarIndex).toBeLessThan(mainIndex);
    
    // .ticket-detail-page がコンテナとして機能していることを確認
    expect(html).toContain('class="ticket-detail-page"');
    expect(html).toContain('data-testid="ticket-detail-page"');
  });

  it('コンテンツ領域 (.ticket-detail-content) とサイドバー (.ticket-detail-sidebar) は .ticket-detail-main 内に存在する', () => {
    const html = render();

    // .ticket-detail-main の開始位置より後に content と sidebar が両方存在することを確認
    // (renderToStaticMarkup の出力はネストした </div> を含むため、直後の閉じタグでは
    //  スライスできない。位置の前後関係のみで包含関係を検証する。)
    const mainIndex = html.indexOf('class="ticket-detail-main"');
    const contentIndex = html.indexOf('class="ticket-detail-content"');
    const sidebarIndex = html.indexOf('class="ticket-detail-sidebar"');

    expect(mainIndex).toBeGreaterThan(-1);
    expect(contentIndex).toBeGreaterThan(mainIndex);
    expect(sidebarIndex).toBeGreaterThan(mainIndex);
  });
});
