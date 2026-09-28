import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import type { TicketDetailView } from '../types/ticketDetailView';

const mockNavigate = vi.fn();
const mockLocalCreateTicket = vi.fn();
const mockBumpTicketCounter = vi.fn();

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom');
  return { ...actual, useNavigate: () => mockNavigate };
});

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('../../../shared/sync/repos/ticketRepo', () => ({
  useSubTickets: () => ({
    data: mockChildTickets,
    isLoading: false,
  }),
}));

vi.mock('../../../shared/sync/ticketWrites', () => ({
  localCreateTicket: (...args: unknown[]) => mockLocalCreateTicket(...args),
  bumpTicketCounter: (...args: unknown[]) => mockBumpTicketCounter(...args),
  isTempTicketKey: (key: string) => key.startsWith('local-'),
}));

vi.mock('../../../shared/sync/ticketMapping', () => ({
  syncStateOf: () => 'synced',
}));

vi.mock('../hooks/useStatusOptions', () => ({
  useStatusOptions: () => ({
    options: [
      { value: 'open', label: 'Todo', color: '#3b82f6' },
      { value: 'closed', label: 'Done', color: '#22c55e' },
    ],
    labelOf: (slug: string) => slug,
  }),
}));

vi.mock('./TicketSubTickets.css', () => ({}));

let mockChildTickets: Array<{
  id: number;
  ticketKey: string;
  title: string;
  status: string;
  assignees: Array<{ id: number; username: string; displayName: string }>;
}> = [];

const parentTicket: TicketDetailView = {
  id: 10,
  ticketKey: 'PRS-100',
  title: 'Parent ticket',
  description: '',
  ticketType: 'task',
  status: 'in_progress',
  priority: 'high',
  assignees: [{ id: 1, username: 'alice', displayName: 'Alice Smith' }],
  reviewers: [],
  author: null,
  category: null,
  milestone: null,
  project: 5,
  projectPrefix: 'PRS',
  projectName: 'パン屋レジシステム',
  team: { id: 7, slug: 'team-7', name: 'Team 7' },
  startDate: null,
  dueDate: null,
  createdAt: '2026-01-01',
  updatedAt: '2026-01-02',
  storyPoints: null,
  cycle: 3,
  cycleName: 'Cycle 5',
  labels: [{ id: 1, name: 'backend', color: '#000' }],
  isWatching: false,
  childCount: 0,
};

import { TicketSubTickets } from './TicketSubTickets';

function render(): string {
  return renderToStaticMarkup(
    createElement(
      MemoryRouter,
      null,
      createElement(TicketSubTickets, { parentTicket }),
    ),
  );
}

describe('TicketSubTickets', () => {
  beforeEach(() => {
    mockChildTickets = [];
    mockNavigate.mockReset();
    mockLocalCreateTicket.mockReset();
    mockBumpTicketCounter.mockReset();
    mockLocalCreateTicket.mockResolvedValue({ tempId: -1, tempKey: 'local-test' });
    mockBumpTicketCounter.mockResolvedValue(undefined);
  });

  it('空状態で追加リンクを表示する', () => {
    const html = render();
    expect(html).toContain('ticket-sub-tickets-add-btn');
    expect(html).toContain('ticketDetail.subTicketsAdd');
    expect(html).not.toContain('ticketDetail.subTicketsEmpty');
  });

  it('サブチケット一覧と進捗バッジを表示する', () => {
    mockChildTickets = [
      {
        id: 11,
        ticketKey: 'PRS-101',
        title: 'Child done',
        status: 'closed',
        assignees: [{ id: 2, username: 'bob', displayName: 'Bob Jones' }],
      },
      {
        id: 12,
        ticketKey: 'PRS-102',
        title: 'Child open',
        status: 'open',
        assignees: [],
      },
    ];
    const html = render();
    expect(html).toContain('ticket-sub-tickets-progress');
    expect(html).toContain('1 / 2');
    expect(html).toContain('PRS-101');
    expect(html).toContain('Child open');
    expect(html).toContain('ticket-sub-ticket-item__assignee');
  });
});
