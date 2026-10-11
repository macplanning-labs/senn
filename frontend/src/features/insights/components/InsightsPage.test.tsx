// @vitest-environment jsdom
/**
 * InsightsPage.test.tsx - Insights 画面のテスト
 *
 * API はモックし、タブ・条件・空の状態・エラー・注記を確かめる。
 * 文言は i18n のキーから取り、ソースに日本語を書かない。
 */

import { describe, it, expect, vi, beforeAll, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import i18n from '@/i18n';
import { apiClient } from '@/shared/api/client';
import { shiftIsoDate, toIsoDate } from '../format';
import { InsightsPage } from './InsightsPage';

vi.mock('@/shared/api/client', () => ({
  apiClient: { get: vi.fn() },
}));

vi.mock('@/shared/sync/repos/projectRepo', () => ({
  useProjects: () => ({
    projects: [{ id: 7, name: 'Alpha', prefix: 'ALP' }],
    isLoading: false,
  }),
}));

const get = vi.mocked(apiClient.get);

const cycleBody = {
  from: '2026-07-10',
  to: '2026-10-08',
  segments: [
    { segment: 'human', count: 4, medianSeconds: 7200, meanSeconds: 8000, p90Seconds: 100000 },
    { segment: 'ai', count: 2, medianSeconds: 1800, meanSeconds: 1800, p90Seconds: 2400 },
    { segment: 'unassigned', count: 0, medianSeconds: null, meanSeconds: null, p90Seconds: null },
  ],
  excluded: { noStart: 3 },
  aiRuns: { unfinished: 1, error: 2 },
};

const emptyCycleBody = {
  ...cycleBody,
  segments: cycleBody.segments.map((s) => ({ ...s, count: 0, medianSeconds: null, meanSeconds: null, p90Seconds: null })),
  excluded: { noStart: 0 },
  aiRuns: { unfinished: 0, error: 0 },
};

const throughputBody = {
  granularity: 'week',
  series: [
    { periodStart: '2026-10-05', segment: 'human', tickets: 3, points: 8 },
    { periodStart: '2026-10-05', segment: 'ai', tickets: 1, points: 2 },
  ],
  aiShare: { ticketsPercent: 25, pointsPercent: 20 },
};

const triageBody = {
  granularity: 'week',
  series: [{ periodStart: '2026-10-05', segment: 'human', count: 2, medianSeconds: 3600 }],
  waiting: { count: 1, medianAgeDays: 2, maxAgeDays: 5 },
};

const aiUsageBody = {
  from: '2026-07-10',
  to: '2026-10-08',
  granularity: 'week',
  totals: {
    runs: 3,
    tickets: 2,
    inputTokens: 1200,
    outputTokens: 800,
    cacheCreationTokens: 5000,
    cacheReadTokens: 90000,
    weightedTokens: 12345,
    usdEquivalent: 1.5,
    avgWeightedPerRun: 4115,
    avgWeightedPerTicket: 6172,
  },
  series: [{ periodStart: '2026-10-05', runs: 3, inputTokens: 1200, outputTokens: 800, cacheCreationTokens: 5000, cacheReadTokens: 90000, weightedTokens: 12345 }],
  excluded: { unfinished: 1, noTokens: 2 },
};

function bodyFor(url: string) {
  if (url.includes('cycle-time')) return cycleBody;
  if (url.includes('triage-time')) return triageBody;
  if (url.includes('throughput')) return throughputBody;
  return aiUsageBody;
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <InsightsPage />
    </QueryClientProvider>,
  );
}

/** 最後に呼ばれたときの、URL とクエリを取り出す */
function lastCall(path: string) {
  const calls = get.mock.calls.filter(([url]) => url === path);
  return calls[calls.length - 1];
}

beforeAll(async () => {
  await i18n.changeLanguage('en');
});

beforeEach(() => {
  get.mockReset();
  get.mockImplementation(async (url: string) => ({ data: bodyFor(url) }) as never);
});

afterEach(() => {
  cleanup();
});

describe('InsightsPage', () => {
  it('loads the cycle time with the default 90-day period when first shown', async () => {
    renderPage();
    const today = toIsoDate(new Date());
    await waitFor(() =>
      expect(get).toHaveBeenCalledWith('/insights/cycle-time/', {
        params: expect.objectContaining({ from: shiftIsoDate(today, -90), to: today, granularity: 'week' }),
      }),
    );
    expect(await screen.findByText('2 tickets')).toBeInTheDocument();
  });

  it('calls only the API of the tab that is opened', async () => {
    renderPage();
    await screen.findByText('2 tickets');
    expect(get.mock.calls.some(([url]) => url === '/insights/throughput/')).toBe(false);

    fireEvent.click(screen.getByRole('tab', { name: 'Throughput' }));
    await waitFor(() => expect(get).toHaveBeenCalledWith('/insights/throughput/', expect.anything()));
    expect(await screen.findByText('25%')).toBeInTheDocument();
  });

  it('sends the period, grouping and ticket type as query parameters', async () => {
    renderPage();
    const today = toIsoDate(new Date());
    await screen.findByText('2 tickets');

    fireEvent.click(screen.getByRole('button', { name: 'Last 30 days' }));
    await waitFor(() =>
      expect(lastCall('/insights/cycle-time/')?.[1]).toEqual({
        params: expect.objectContaining({ from: shiftIsoDate(today, -30), to: today }),
      }),
    );

    fireEvent.click(screen.getByRole('button', { name: 'Monthly' }));
    await waitFor(() =>
      expect(lastCall('/insights/cycle-time/')?.[1]).toEqual({
        params: expect.objectContaining({ granularity: 'month' }),
      }),
    );

    fireEvent.change(screen.getByLabelText('Ticket type'), { target: { value: 'bug' } });
    await waitFor(() =>
      expect(lastCall('/insights/cycle-time/')?.[1]).toEqual({
        params: expect.objectContaining({ ticket_type: 'bug' }),
      }),
    );
  });

  it('shows the empty-state explanation when there is no data', async () => {
    get.mockImplementation(async () => ({ data: emptyCycleBody }) as never);
    renderPage();
    expect(await screen.findByText(i18n.t('insights.empty.body'))).toBeInTheDocument();
  });

  it('shows the detail returned by the API and a retry button on a 400 error', async () => {
    get.mockRejectedValue({ response: { status: 400, data: { detail: 'Range is too long' } } });
    renderPage();
    expect(await screen.findByText('Range is too long')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: i18n.t('insights.error.retry') })).toBeInTheDocument();
  });

  it('shows the API-equivalent note on the AI usage tab', async () => {
    renderPage();
    fireEvent.click(screen.getByRole('tab', { name: 'AI usage' }));
    expect(await screen.findByText(i18n.t('insights.usd.note'))).toBeInTheDocument();
  });

  it('shows the excluded counts as notes instead of hiding them', async () => {
    renderPage();
    expect(await screen.findByText(i18n.t('insights.note.noStart', { n: 3 }))).toBeInTheDocument();
    expect(screen.getByText(i18n.t('insights.note.aiError', { n: 2 }))).toBeInTheDocument();
  });
});
