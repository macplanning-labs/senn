// @vitest-environment jsdom
/**
 * ConnectPage.test.tsx — MCP ペアリング確認ページの画面テスト(DEMO-000192)
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter, Routes, Route } from 'react-router-dom';
import i18n from 'i18next';
import { ConnectPage } from './ConnectPage';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';

vi.mock('@/shared/api/client');
vi.mock('@/shared/stores/toastStore');

const mockApi = apiClient as unknown as {
  get: ReturnType<typeof vi.fn>;
  post: ReturnType<typeof vi.fn>;
};
const mockToast = useToast as unknown as ReturnType<typeof vi.fn>;

if (!i18n.isInitialized) {
  void i18n.init({
    lng: 'ja',
    fallbackLng: 'ja',
    resources: {
      ja: {
        translation: {
          'common.loading': 'ロード中…',
          'common.error': 'エラーが発生しました',
          'aiMcp.connectPageTitle': 'AI Agent を使うことを申請する',
          'aiMcp.connectDescription': '`{{label}}` で AI Agent(Claude など)を使えるように、管理者に許可を求めます。',
          'aiMcp.connectRequestButton': '許可を求める',
          'aiMcp.connectRequestedMessage': '許可を求めました。承認されると、自動で使えるようになります。この画面は閉じて構いません。',
          'aiMcp.connectApprovedMessage': '使えるようになりました。Claude から、そのまま使えます。',
          'aiMcp.connectRejectedMessage': '許可されませんでした。',
          'aiMcp.connectFailedMessage': '接続できませんでした。',
          'aiMcp.connectExpiredLink': 'このリンクは、期限切れか、無効です。Claude で、もう一度やり直してください。',
          'aiMcp.connectInvalidLink': 'リンクが正しくありません。',
          'aiMcp.connectReasonPrefix': '理由: ',
          'aiMcp.connectApplyFailed': '申請に失敗しました',
        },
      },
    },
  });
}

function renderWithRouter(ui: React.ReactElement, initialRoute = '/connect?code=ABCD-EFGH') {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <I18nextProvider i18n={i18n}>
        <MemoryRouter initialEntries={[initialRoute]}>
          <Routes>
            <Route path="/connect" element={ui} />
          </Routes>
        </MemoryRouter>
      </I18nextProvider>
    </QueryClientProvider>,
  );
}

describe('ConnectPage', () => {
  beforeEach(() => {
    mockToast.mockReturnValue({ success: vi.fn(), error: vi.fn() });
    mockApi.get = vi.fn();
    mockApi.post = vi.fn();
  });
  afterEach(() => cleanup());

  it('code が無いと、エラーを表示する', () => {
    renderWithRouter(<ConnectPage />, '/connect');
    expect(screen.getByText('リンクが正しくありません。')).toBeInTheDocument();
  });

  it('waiting ステータスで label と ボタンを表示する', async () => {
    mockApi.get.mockResolvedValue({
      data: {
        code: 'ABCD-EFGH',
        label: 'My MacBook',
        status: 'waiting',
        message: null,
        createdAt: '2026-10-08T00:00:00Z',
      },
    });

    renderWithRouter(<ConnectPage />);
    await waitFor(() => {
      expect(screen.getByText(/`My MacBook` で AI Agent/)).toBeInTheDocument();
    });

    expect(screen.getByTestId('connect-apply-btn')).toBeInTheDocument();
    expect(screen.getByText('許可を求める')).toBeInTheDocument();
  });

  it('waiting でボタンを押すと apply を呼ぶ', async () => {
    const user = userEvent.setup();
    mockApi.get.mockResolvedValue({
      data: {
        code: 'ABCD-EFGH',
        label: 'My MacBook',
        status: 'waiting',
        message: null,
        createdAt: '2026-10-08T00:00:00Z',
      },
    });
    mockApi.post.mockResolvedValue({
      data: {
        code: 'ABCD-EFGH',
        label: 'My MacBook',
        status: 'requested',
        message: null,
        createdAt: '2026-10-08T00:00:00Z',
      },
    });

    renderWithRouter(<ConnectPage />);
    const btn = await screen.findByTestId('connect-apply-btn');
    await user.click(btn);

    expect(mockApi.post).toHaveBeenCalledWith('/ai-mcp/pairings/ABCD-EFGH/apply/');
  });

  it('requested ステータスで案内文を表示する', async () => {
    mockApi.get.mockResolvedValue({
      data: {
        code: 'ABCD-EFGH',
        label: 'My MacBook',
        status: 'requested',
        message: null,
        createdAt: '2026-10-08T00:00:00Z',
      },
    });

    renderWithRouter(<ConnectPage />);
    await waitFor(() => {
      expect(
        screen.getByText('許可を求めました。承認されると、自動で使えるようになります。この画面は閉じて構いません。'),
      ).toBeInTheDocument();
    });

    expect(screen.queryByTestId('connect-apply-btn')).not.toBeInTheDocument();
  });

  it('approved ステータスで完了文を表示する', async () => {
    mockApi.get.mockResolvedValue({
      data: {
        code: 'ABCD-EFGH',
        label: 'My MacBook',
        status: 'approved',
        message: null,
        createdAt: '2026-10-08T00:00:00Z',
      },
    });

    renderWithRouter(<ConnectPage />);
    await waitFor(() => {
      expect(screen.getByText('使えるようになりました。Claude から、そのまま使えます。')).toBeInTheDocument();
    });
  });

  it('rejected ステータスで却下文と理由を表示する', async () => {
    mockApi.get.mockResolvedValue({
      data: {
        code: 'ABCD-EFGH',
        label: 'My MacBook',
        status: 'rejected',
        message: 'Not approved by policy',
        createdAt: '2026-10-08T00:00:00Z',
      },
    });

    renderWithRouter(<ConnectPage />);
    await waitFor(() => {
      expect(screen.getByText('許可されませんでした。')).toBeInTheDocument();
      expect(screen.getByText(/理由: Not approved by policy/)).toBeInTheDocument();
    });
  });

  it('failed ステータスで失敗メッセージを表示する', async () => {
    mockApi.get.mockResolvedValue({
      data: {
        code: 'ABCD-EFGH',
        label: 'My MacBook',
        status: 'failed',
        message: 'Key limit exceeded',
        createdAt: '2026-10-08T00:00:00Z',
      },
    });

    renderWithRouter(<ConnectPage />);
    await waitFor(() => {
      expect(screen.getByText('接続できませんでした。')).toBeInTheDocument();
      expect(screen.getByText(/理由: Key limit exceeded/)).toBeInTheDocument();
    });
  });

  it('expired ステータスで期限切れメッセージを表示する', async () => {
    mockApi.get.mockResolvedValue({
      data: {
        code: 'ABCD-EFGH',
        label: 'My MacBook',
        status: 'expired',
        message: null,
        createdAt: '2026-10-08T00:00:00Z',
      },
    });

    renderWithRouter(<ConnectPage />);
    await waitFor(() => {
      expect(screen.getByText(/このリンクは、期限切れか、無効です/)).toBeInTheDocument();
    });
  });

  it('404 エラーで期限切れメッセージを表示する', async () => {
    mockApi.get.mockRejectedValue({
      response: {
        status: 404,
        data: { error: 'Not found' },
      },
    });

    renderWithRouter(<ConnectPage />);
    await waitFor(() => {
      expect(screen.getByText(/このリンクは、期限切れか、無効です/)).toBeInTheDocument();
    });
  });
});
