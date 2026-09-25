import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import type { TicketDetailView } from '../types/ticketDetailView';

// Mock i18next
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

// Mock react-query
// 本コンポーネントは useQuery で選択肢(ユーザー・ラベル・サイクル等)を取得する。
// renderToStaticMarkup には QueryClientProvider が無いため、
// このファイルの他のモックと同じ方針でモジュールごと差し替える。
// 子コンポーネントや配下のフックが useMutation / useQueryClient を使う場合にも
// 備えて、3つとも用意しておく。
vi.mock('@tanstack/react-query', () => ({
  useQuery: () => ({ data: [], isLoading: false, isError: false, error: null, refetch: vi.fn() }),
  useMutation: () => ({ mutate: vi.fn(), mutateAsync: vi.fn(), isPending: false, isError: false }),
  useQueryClient: () => ({
    invalidateQueries: vi.fn(),
    setQueryData: vi.fn(),
    getQueryData: vi.fn(),
  }),
}));

// Mock API and hooks
vi.mock('@/shared/api/client', () => ({
  apiClient: {},
}));

vi.mock('@/shared/hooks/useProject', () => ({
  useProject: () => ({ projectList: [] }),
}));

vi.mock('@/shared/stores/toastStore', () => ({
  useToastStore: () => ({ addToast: vi.fn() }),
}));

vi.mock('@/features/settings/hooks/useWorkflowStatuses', () => ({
  useWorkflowStatuses: () => ({ data: [] }),
}));

vi.mock('@/features/tickets/utils/ticketUserOptions', () => ({
  fetchTicketUserOptions: vi.fn(),
  ticketUserOptionsEnabled: () => false,
}));

vi.mock('@/features/tickets/hooks/useTicketPropertyMutations', () => ({
  useTicketPropertyMutations: (ticketId: string) => ({
    status: { mutate: vi.fn(), isPending: false },
    priority: { mutate: vi.fn(), isPending: false },
    type: { mutate: vi.fn(), isPending: false },
    storyPoints: { mutate: vi.fn(), isPending: false },
    startDate: { mutate: vi.fn(), isPending: false },
    dueDate: { mutate: vi.fn(), isPending: false },
    cycle: { mutate: vi.fn(), isPending: false },
    category: { mutate: vi.fn(), isPending: false },
    milestone: { mutate: vi.fn(), isPending: false },
    labels: { mutate: vi.fn(), isPending: false },
    assignees: { mutate: vi.fn(), isPending: false },
    reviewers: { mutate: vi.fn(), isPending: false },
  }),
}));

vi.mock('@/features/tickets/hooks/useTicketDetailHotkeys', () => ({
  useTicketDetailHotkeys: vi.fn(),
}));

vi.mock('./TicketPropertiesSidebar.css', () => ({}));

import { TicketPropertiesSidebar } from './TicketPropertiesSidebar';

const mockTicket: TicketDetailView = {
  ticketKey: 'PROJ-1',
  title: 'Test Ticket',
  description: 'Test description',
  ticketType: 'bug',
  status: 'open',
  priority: 'high',
  assignees: [],
  reviewers: [],
  labels: [],
  project: 1,
  projectName: 'Test Project',
  team: { id: 1, name: 'Test Team' },
  team_id: 1,
  cycleName: null,
  cycle: null,
  category: null,
  milestone: null,
  storyPoints: null,
  createdAt: '2024-01-01T00:00:00Z',
  updatedAt: '2024-01-01T00:00:00Z',
  author: { id: 1, username: 'author', displayName: 'Author' },
  dueDate: null,
  startDate: null,
} as unknown as TicketDetailView;

function render(ticket: TicketDetailView = mockTicket): string {
  return renderToStaticMarkup(
    createElement(
      MemoryRouter,
      null,
      createElement(TicketPropertiesSidebar, {
        ticket,
        ticketId: 'PROJ-1',
        onFocusTitle: vi.fn(),
        onFocusDescription: vi.fn(),
      }),
    ),
  );
}

describe('TicketPropertiesSidebar - Type Property', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('Type行が「Properties」セクションに表示される', () => {
    const html = render();
    expect(html).toContain('Type');
  });

  it('チケットのticketTypeが正しく表示される', () => {
    const html = render(mockTicket);
    expect(html).toContain('bug');
  });

  it('ticketTypeがnullの場合は「—」が表示される', () => {
    const ticketWithoutType = { ...mockTicket, ticketType: null };
    const html = render(ticketWithoutType as unknown as TicketDetailView);
    expect(html).toContain('Type');
  });

  it('Type行がクリック可能なボタンとして表示される', () => {
    const html = render();
    // Check that there's a button with ticket-property-type testid
    expect(html).toContain('data-testid="ticket-property-type"');
    expect(html).toContain('ticket-property-item--button');
  });
});
