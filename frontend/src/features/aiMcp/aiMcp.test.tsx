// @vitest-environment jsdom
/**
 * AI Agent を利用した MCP 操作の申請(DEMO-000192)の画面テスト
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { AiMcpAccessSection } from './AiMcpAccessSection';
import { AdminAiMcpRequestsSection } from './AdminAiMcpRequestsSection';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import type { AiMcpAdminList, AiMcpMine, AiMcpRequest } from './types';

vi.mock('@/shared/api/client');
vi.mock('@/shared/stores/toastStore');

const mockApi = apiClient as unknown as {
  get: ReturnType<typeof vi.fn>;
  post: ReturnType<typeof vi.fn>;
  delete: ReturnType<typeof vi.fn>;
};
const mockToast = useToast as unknown as ReturnType<typeof vi.fn>;

if (!i18n.isInitialized) {
  void i18n.init({
    lng: 'ja',
    fallbackLng: 'ja',
    resources: {
      ja: {
        translation: {
          'aiMcp.title': 'MCP操作',
          'aiMcp.statusConnected': '接続済み({{count}} 台)',
          'aiMcp.statusNotConnected': 'まだ接続していません',
          'aiMcp.hintConnect': 'Claude に頼んでください',
          'aiMcp.detailsSummary': '詳細(手動の操作)',
          'aiMcp.statusNone': 'まだ申請していません。',
          'aiMcp.statusPending': '承認待ち',
          'aiMcp.statusApproved': '承認済み',
          'aiMcp.statusRejected': '却下',
          'aiMcp.statusIssued': 'キー発行済み',
          'aiMcp.statusCancelled': '取り下げ',
          'aiMcp.statusExpired': '期限切れ',
          'aiMcp.requestButton': '利用を申請する',
          'aiMcp.issueButton': 'キーを発行する',
          'aiMcp.cancelButton': '取り下げる',
          'aiMcp.approve': '承認する',
          'aiMcp.reject': '却下する',
          'aiMcp.keyShownOnce': '今だけ表示',
          'aiMcp.noKeys': 'キーはまだありません。',
          'aiMcp.noRequests': '申請はありません。',
          'aiMcp.rejectedHint': '却下されました。',
          'aiMcp.reasonPrefix': '理由: ',
        },
      },
    },
  });
}

function req(over: Partial<AiMcpRequest>): AiMcpRequest {
  return {
    id: 1,
    user: { id: 3, username: 'alice', email: 'a@x', displayName: 'アリス' },
    purpose: 'テスト',
    status: 'pending',
    decidedBy: null,
    decidedAt: null,
    decisionNote: '',
    approvalExpiresAt: null,
    issuedAt: null,
    createdAt: '2026-10-08T00:00:00Z',
    ...over,
  };
}

function mine(over: Partial<AiMcpMine>): AiMcpMine {
  return { requests: [], keys: [], maxActiveKeys: 3, approvalValidDays: 7, ...over };
}

function renderWith(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <I18nextProvider i18n={i18n}>{ui}</I18nextProvider>
    </QueryClientProvider>,
  );
}

describe('AiMcpAccessSection(本人)', () => {
  beforeEach(() => {
    mockToast.mockReturnValue({ success: vi.fn(), error: vi.fn() });
    mockApi.get = vi.fn();
    mockApi.post = vi.fn();
    mockApi.delete = vi.fn();
  });
  afterEach(() => cleanup());

  it('申請が無いとき、申請ボタンだけが出て(入力欄は出ない)、押すと申請する', async () => {
    mockApi.get.mockResolvedValue({ data: mine({}) });
    mockApi.post.mockResolvedValue({ data: req({}) });
    renderWith(<AiMcpAccessSection />);
    const btn = await screen.findByTestId('ai-mcp-request');
    expect(screen.queryByRole('textbox')).toBeNull();
    await userEvent.click(btn);
    await waitFor(() => expect(mockApi.post).toHaveBeenCalledWith('/ai-mcp/requests/', {}));
  });

  it('承認待ちのときは、申請ボタンもキー発行ボタンも出ない(取り下げだけ)', async () => {
    mockApi.get.mockResolvedValue({ data: mine({ requests: [req({ status: 'pending' })] }) });
    renderWith(<AiMcpAccessSection />);
    expect(await screen.findByTestId('ai-mcp-latest')).toHaveAttribute('data-status', 'pending');
    expect(screen.queryByTestId('ai-mcp-request')).toBeNull();
    expect(screen.queryByTestId('ai-mcp-issue')).toBeNull();
    expect(screen.getByTestId('ai-mcp-cancel')).toBeInTheDocument();
  });

  it('承認済みなら、キーを発行でき、発行したキーは画面に表示される', async () => {
    mockApi.get.mockResolvedValue({ data: mine({ requests: [req({ status: 'approved', approvalExpiresAt: '2026-10-15T00:00:00Z' })] }) });
    mockApi.post.mockResolvedValue({ data: { id: 9, plainKey: 'senn_ai_SECRETVALUE', keyPrefix: 'senn_ai_SECR' } });
    renderWith(<AiMcpAccessSection />);
    await userEvent.click(await screen.findByTestId('ai-mcp-issue'));
    expect(await screen.findByTestId('ai-mcp-key-issued')).toHaveTextContent('senn_ai_SECRETVALUE');
    expect(mockApi.post).toHaveBeenCalledWith('/ai-mcp/requests/1/issue/', { label: undefined });
  });

  it('有効なキーが上限に達していると、発行ボタンは押せない', async () => {
    const keys = [1, 2, 3].map((i) => ({ id: i, keyPrefix: `senn_ai_${i}`, label: null, createdAt: '2026-10-01T00:00:00Z', lastUsedAt: null, revokedAt: null }));
    mockApi.get.mockResolvedValue({ data: mine({ keys, requests: [req({ status: 'approved' })] }) });
    renderWith(<AiMcpAccessSection />);
    expect(await screen.findByTestId('ai-mcp-issue')).toBeDisabled();
  });

  it('却下されたときは理由を表示し、もう一度申請できる', async () => {
    mockApi.get.mockResolvedValue({ data: mine({ requests: [req({ status: 'rejected', decisionNote: '今回は見送り' })] }) });
    renderWith(<AiMcpAccessSection />);
    expect(await screen.findByTestId('ai-mcp-latest')).toHaveTextContent('今回は見送り');
    expect(screen.getByTestId('ai-mcp-request')).toBeInTheDocument();
  });

  it('キーがあれば「接続済み」の1行だけが見え、キーの一覧などは閉じた「詳細」の中にある', async () => {
    const keys = [{ id: 1, keyPrefix: 'senn_ai_abcd', label: 'MacBook', createdAt: '2026-10-08T00:00:00Z', lastUsedAt: null, revokedAt: null }];
    mockApi.get.mockResolvedValue({ data: mine({ keys, requests: [req({ status: 'issued' })] }) });
    renderWith(<AiMcpAccessSection />);
    await waitFor(() =>
      expect(screen.getByTestId('ai-mcp-status')).toHaveAttribute('data-state', 'connected'),
    );
    expect(screen.getByTestId('ai-mcp-status')).toHaveTextContent('接続済み(1 台)');
    const details = screen.getByTestId('ai-mcp-details') as HTMLDetailsElement;
    expect(details.open).toBe(false);
    expect(details.contains(screen.getByTestId('ai-mcp-key-1'))).toBe(true);
  });

  it('接続も申請も無いときは「まだ接続していません」と案内だけを出す', async () => {
    mockApi.get.mockResolvedValue({ data: mine({}) });
    renderWith(<AiMcpAccessSection />);
    await waitFor(() => expect(mockApi.get).toHaveBeenCalled());
    const status = await screen.findByTestId('ai-mcp-status');
    expect(status).toHaveAttribute('data-state', 'none');
    expect(status).toHaveTextContent('まだ接続していません');
    expect(status).toHaveTextContent('Claude に頼んでください');
  });
});

describe('AdminAiMcpRequestsSection(管理者)', () => {
  beforeEach(() => {
    mockToast.mockReturnValue({ success: vi.fn(), error: vi.fn() });
    mockApi.get = vi.fn();
    mockApi.post = vi.fn();
  });
  afterEach(() => cleanup());

  it('承認待ちの申請を、承認・却下できる(コメントつき)', async () => {
    const list: AiMcpAdminList = { requests: [req({ id: 5 })], pendingCount: 1 };
    mockApi.get.mockResolvedValue({ data: list });
    mockApi.post.mockResolvedValue({ data: req({ id: 5, status: 'approved' }) });
    renderWith(<AdminAiMcpRequestsSection />);
    await userEvent.type(await screen.findByRole('textbox'), '了解');
    await userEvent.click(screen.getByTestId('ai-mcp-approve-5'));
    await waitFor(() =>
      expect(mockApi.post).toHaveBeenCalledWith('/system-admin/ai-mcp-requests/5/approve/', { note: '了解' }),
    );
    await userEvent.click(screen.getByTestId('ai-mcp-reject-5'));
    await waitFor(() =>
      expect(mockApi.post).toHaveBeenCalledWith('/system-admin/ai-mcp-requests/5/reject/', { note: '了解' }),
    );
  });

  it('処理済みの申請には、承認・却下ボタンが出ない', async () => {
    mockApi.get.mockResolvedValue({
      data: { requests: [req({ id: 6, status: 'issued', decidedBy: { id: 1, username: 'admin', email: 'x', displayName: '管理者' } })], pendingCount: 0 },
    });
    renderWith(<AdminAiMcpRequestsSection />);
    await screen.findByTestId('ai-mcp-request-6');
    expect(screen.queryByTestId('ai-mcp-approve-6')).toBeNull();
  });

  it('申請が無いときは、その旨を表示する', async () => {
    mockApi.get.mockResolvedValue({ data: { requests: [], pendingCount: 0 } });
    renderWith(<AdminAiMcpRequestsSection />);
    expect(await screen.findByText('申請はありません。')).toBeInTheDocument();
  });

  it('承認する人に、誰が・どの Mac で申請したかが分かる(申請者の下にMacの名前)', async () => {
    mockApi.get.mockResolvedValue({
      data: { requests: [req({ id: 8, purpose: '端末の接続: アリスのMacBook' })], pendingCount: 1 },
    });
    renderWith(<AdminAiMcpRequestsSection />);
    expect(await screen.findByTestId('ai-mcp-device-8')).toHaveTextContent('アリスのMacBook');
    expect(screen.getByTestId('ai-mcp-request-8')).toHaveTextContent('アリス');
  });
});
