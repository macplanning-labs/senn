// @vitest-environment jsdom
/**
 * ProjectAiKickSettings.test.tsx
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { I18nextProvider } from 'react-i18next';
import { BrowserRouter } from 'react-router-dom';
import i18n from 'i18next';
import { ProjectAiKickSettings } from './ProjectAiKickSettings';
import { apiClient } from '@/shared/api/client';
import { useToastStore } from '@/shared/stores/toastStore';

// Mock deps
vi.mock('@/shared/api/client');
vi.mock('@/shared/stores/toastStore');

const mockApiClient = apiClient as any;
const mockToastStore = useToastStore as any;

// Initialize i18n for tests with complete resources
if (!i18n.isInitialized) {
  void i18n.init({
    lng: 'ja',
    fallbackLng: 'ja',
    resources: {
      ja: {
        translation: {
          'projectSettings.aiKick': 'AI 自動実行',
          'projectSettings.aiKickEnable': '有効にする',
          'projectSettings.aiKickSystemDisabled': 'システムでまだ有効にされていません(管理者が設定します)',
          'projectSettings.aiKickSystemKilled': '緊急停止中です',
          'projectSettings.aiKickRepo': 'リポジトリ（owner/name）',
          'projectSettings.aiKickBranch': 'ベースのブランチ',
          'projectSettings.aiKickBranchDefault': '既定 develop',
          'projectSettings.aiKickReadOnly': '変更できるのはプロジェクトのオーナーと管理者です',
          'projectSettings.aiKickRepoRequired': 'リポジトリが必須です',
          'projectSettings.aiKickRepoSelect': '選んでください',
          'projectSettings.aiKickRepoNoneAllowed': 'AI の作業を許可されたリポジトリがありません',
          'projectSettings.aiKickRepoNotAllowed': '(許可されていません)',
          'projectSettings.aiKickRepoHint': 'システム管理者が許可したリポジトリから選びます',
          'projectSettings.aiKickDataSharing': 'Anthropic 社に送られることを理解しました',
          'projectSettings.aiKickDataSharingRequired': 'Anthropic 社に送られることの確認にチェックを入れてください',
          'projectSettings.aiKickSaved': 'AI 自動実行の設定が保存されました',
          'projectSettings.aiKickSaveFailed': 'AI 自動実行の設定の保存に失敗しました',
          'common.cancel': 'キャンセル',
          'common.close': '閉じる',
          'common.save': '保存',
          'common.saving': '保存中...',
          'common.loading': '読み込み中...',
        },
      },
    },
    react: {
      useSuspense: false,
    },
  });
}

const mockAiKickSettings = {
  enabled: false,
  repo: 'owner/repo',
  baseBranch: 'develop',
  canEdit: true,
  systemEnabled: true,
  systemKilled: false,
  allowedRepos: ['owner/repo', 'newowner/newrepo'],
};

function setupTest() {
  const queryClient = new QueryClient();
  const mockAddToast = vi.fn();

  mockToastStore.mockReturnValue({
    addToast: mockAddToast,
  });

  mockApiClient.get.mockResolvedValue({ data: mockAiKickSettings });
  mockApiClient.put.mockResolvedValue({ data: mockAiKickSettings });

  return {
    queryClient,
    mockAddToast,
  };
}

function renderComponent(queryClient: QueryClient) {
  return render(
    <QueryClientProvider client={queryClient}>
      <I18nextProvider i18n={i18n}>
        <BrowserRouter>
          <ProjectAiKickSettings projectId="proj-1" />
        </BrowserRouter>
      </I18nextProvider>
    </QueryClientProvider>
  );
}

describe('ProjectAiKickSettings', () => {
  afterEach(() => cleanup());

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders readonly message when canEdit is false', async () => {
    const { queryClient } = setupTest();
    mockApiClient.get.mockResolvedValue({
      data: { ...mockAiKickSettings, canEdit: false },
    });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByText(/変更できるのはプロジェクトのオーナー/i)).toBeInTheDocument();
    });
  });

  it('リポジトリは、管理者が許可した一覧から選ぶ(自由入力はできない)', async () => {
    const { queryClient } = setupTest();
    renderComponent(queryClient);
    const select = (await screen.findByLabelText(/リポジトリ/)) as HTMLSelectElement;
    expect(select.tagName).toBe('SELECT');
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toEqual(['', 'owner/repo', 'newowner/newrepo']);
    expect(screen.getByText(/許可したリポジトリから選びます/)).toBeInTheDocument();
  });

  it('許可されたリポジトリが無ければ、管理者が許可することを案内する', async () => {
    const { queryClient } = setupTest();
    mockApiClient.get.mockResolvedValue({ data: { ...mockAiKickSettings, repo: '', allowedRepos: [] } });
    renderComponent(queryClient);
    expect(await screen.findByText(/許可されたリポジトリがありません/)).toBeInTheDocument();
  });

  it('いま設定されているリポジトリが一覧から外れていたら、そうと分かるように出す', async () => {
    const { queryClient } = setupTest();
    mockApiClient.get.mockResolvedValue({ data: { ...mockAiKickSettings, allowedRepos: ['newowner/newrepo'] } });
    renderComponent(queryClient);
    expect(await screen.findByText(/owner\/repo \(許可されていません\)/)).toBeInTheDocument();
  });

  it('requires repo when enabling', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();

    mockApiClient.get.mockResolvedValue({
      data: { ...mockAiKickSettings, repo: '' },
    });

    renderComponent(queryClient);
    await screen.findByLabelText(/リポジトリ/);

    await user.click(screen.getByRole('checkbox', { name: /有効にする/ }));
    await user.click(screen.getByTestId('ai-kick-data-sharing-ack'));
    await user.click(screen.getByText(/保存/i));

    await waitFor(() => {
      expect(screen.getByText(/リポジトリが必須/i)).toBeInTheDocument();
    });
    expect(mockApiClient.put).not.toHaveBeenCalled();
  });

  it('有効にするときは、Anthropic 社に送られることの確認が無いと保存しない', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();
    renderComponent(queryClient);
    await screen.findByLabelText(/リポジトリ/);

    expect(screen.queryByTestId('ai-kick-data-sharing-ack')).not.toBeInTheDocument();
    await user.click(screen.getByRole('checkbox', { name: /有効にする/ }));
    expect(screen.getByTestId('ai-kick-data-sharing-ack')).toBeInTheDocument();
    await user.click(screen.getByText(/保存/i));

    // エラーは、確認のチェックのすぐ下に出る
    const err = await screen.findByTestId('ai-kick-data-sharing-error');
    expect(err.textContent).toMatch(/確認にチェックを入れてください/);
    expect(mockApiClient.put).not.toHaveBeenCalled();
    // チェックを入れると、エラーは消える
    await user.click(screen.getByTestId('ai-kick-data-sharing-ack'));
    expect(screen.queryByTestId('ai-kick-data-sharing-error')).not.toBeInTheDocument();
  });

  it('shows system disabled message', async () => {
    const { queryClient } = setupTest();
    mockApiClient.get.mockResolvedValue({
      data: { ...mockAiKickSettings, systemEnabled: false },
    });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByText(/システムでまだ有効にされていません/i)).toBeInTheDocument();
    });
  });

  it('shows system killed message', async () => {
    const { queryClient } = setupTest();
    mockApiClient.get.mockResolvedValue({
      data: { ...mockAiKickSettings, systemKilled: true },
    });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByText(/緊急停止中です/i)).toBeInTheDocument();
    });
  });

  it('disables form when canEdit is false', async () => {
    const { queryClient } = setupTest();
    mockApiClient.get.mockResolvedValue({
      data: { ...mockAiKickSettings, canEdit: false },
    });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByRole('checkbox')).toBeDisabled();
    });
  });

  it('only enables save when form changed', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();

    renderComponent(queryClient);
    const select = await screen.findByLabelText(/リポジトリ/);

    const saveBtn = screen.getByText(/保存/i) as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(true);

    await user.selectOptions(select, 'newowner/newrepo');
    await waitFor(() => {
      expect(saveBtn.disabled).toBe(false);
    });
  });

  it('sends PUT request with correct data', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();

    renderComponent(queryClient);
    const select = await screen.findByLabelText(/リポジトリ/);

    await user.click(screen.getByRole('checkbox', { name: /有効にする/ }));
    await user.selectOptions(select, 'newowner/newrepo');
    await user.click(screen.getByTestId('ai-kick-data-sharing-ack'));
    await user.click(screen.getByText(/保存/i));

    await waitFor(() => {
      expect(mockApiClient.put).toHaveBeenCalledWith(
        '/projects/proj-1/ai-kick/',
        {
          enabled: true,
          repo: 'newowner/newrepo',
          baseBranch: 'develop',
          dataSharingAcknowledged: true,
        }
      );
    });
  });

  it('displays server error message', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();

    mockApiClient.put.mockRejectedValue({
      response: {
        data: { detail: 'Repository not found' },
      },
    });

    renderComponent(queryClient);
    const select = await screen.findByLabelText(/リポジトリ/);

    await user.selectOptions(select, 'newowner/newrepo');
    await user.click(screen.getByText(/保存/i));

    await waitFor(() => {
      expect(screen.getByText('Repository not found')).toBeInTheDocument();
    });
  });
});
