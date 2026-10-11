// @vitest-environment jsdom
/**
 * AdminAiKickSection.test.tsx
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { AdminAiKickSection } from './AdminAiKickSection';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';

// Mock deps
vi.mock('@/shared/api/client');
vi.mock('@/shared/stores/toastStore');

const mockApiClient = apiClient as any;
const mockToast = useToast as any;

// Initialize i18n for tests with complete resources
if (!i18n.isInitialized) {
  void i18n.init({
    lng: 'ja',
    fallbackLng: 'ja',
    resources: {
      ja: {
        translation: {
          'admin.navAiKick': 'AI 自動実行',
          'admin.aiKickTitle': 'AI 自動実行の設定',
          'admin.aiKickStatus': '状態',
          'admin.aiKickEnabled': '有効',
          'admin.aiKickDisabled': '無効',
          'admin.aiKickKilled': '緊急停止中',
          'admin.aiKickRunnersOnline': '接続中のランナー',
          'admin.aiKickUsageToday': '今日の使用量',
          'admin.aiKickUsageWeek': '直近 7 日間の使用量',
          'admin.aiKickEmergencyStop': '緊急停止する',
          'admin.aiKickEmergencyResume': '緊急停止を解除する',
          'admin.aiKickEmergencyConfirmStop': 'すべての AI 自動実行を止めます。実行中のものも、数十秒以内に止まります',
          'admin.aiKickEmergencyConfirmResume': '原因を確認してから解除してください',
          'admin.aiKickEnable': '有効にする',
          'admin.aiKickEnableDisabledReason': '上限が未保存のため、有効にできません',
          'admin.aiKickLimitsTitle': '上限',
          'admin.aiKickLimitsPerExecution': '1 実行の上限',
          'admin.aiKickMaxTurns': '最大ターン数',
          'admin.aiKickMaxMinutes': '最大の実時間（分）',
          'admin.aiKickMaxTokens': '最大トークン数',
          'admin.aiKickBudget': '予算',
          'admin.aiKickDailyBudgetSystem': 'システム全体の 1 日',
          'admin.aiKickDailyBudgetProject': 'プロジェクトごとの 1 日',
          'admin.aiKickWeeklyBudgetSystem': 'システム全体の 1 週間',
          'admin.aiKickBudgetDesc': '人の作業と同じ利用枠を使います。予算の 80% で警告、100% を超えると確認つきで続行、上限(下の線)に達すると止まります',
          'admin.aiKickFrequency': '回数',
          'admin.aiKickPerUserHour': '1 人あたり 1 時間',
          'admin.aiKickPerUserDay': '1 人あたり 1 日',
          'admin.aiKickPerTicket24h': '1 チケットあたり 24 時間',
          'admin.aiKickMaxQueue': '順番待ちの最大',
          'admin.aiKickRunaway': '暴走の検知',
          'admin.aiKickBurnRate': '1 分あたりのトークン',
          'admin.aiKickMaxFailures': '連続の失敗で緊急停止する回数',
          'admin.aiKickThresholds': '上限の線',
          'admin.aiKickHardCeiling': '予算の何 % で止めるか（既定 200）',
          'admin.aiKickEmergencyCap': '緊急の実行の上限の倍率（既定 200）',
          'admin.aiKickOverrideAdminOnly': '予算を超える確認を管理者だけに許す',
          'admin.aiKickTokenDesc': '加重トークン = 入力 + 出力 + キャッシュ作成 + キャッシュ読み取りの 1/10。目安: 普通の 1 件の作業は 1M 前後、大きくても 13M 程度(2026-10 の実測)',
          'admin.aiKickDefaultButton': '初期値を入れる',
          'admin.aiKickSaveButton': '保存',
          'admin.aiKickRunnersTitle': 'ランナー',
          'admin.aiKickRunnerName': '名前',
          'admin.aiKickRunnerToken': 'トークン',
          'admin.aiKickRunnerOnline': '接続中',
          'admin.aiKickRunnerOffline': '未接続',
          'admin.aiKickRunnerLastSeen': '最後の接続',
          'admin.aiKickRunnerRevoked': '失効済み',
          'admin.aiKickAddRunner': 'ランナーを追加',
          'admin.aiKickRunnerCreatedModalTitle': 'ランナーの認証情報',
          'admin.aiKickRunnerCreatedModalWarning': 'この画面を閉じると、二度と表示されません。ランナーの設定に貼り付けて、安全な場所に保管してください',
          'admin.aiKickRunnerCreatedModalToken': 'トークン',
          'admin.aiKickRunnerCreatedModalSecret': '署名用の秘密',
          'admin.aiKickRunnerCreatedModalCopy': 'コピー',
          'admin.aiKickRunnerCreatedModalSaved': '保管しました',
          'admin.aiKickRunnerRevokedMessage': 'ランナーが失効しました',
          'admin.aiKickRunnerRevokeFailed': 'ランナーの失効に失敗しました',
          'admin.aiKickRunnerRevokeButton': '失効させる',
          'admin.aiKickRunnerRevokeConfirm': '実行中の依頼があれば、停止を求めます',
          'admin.aiKickSetupTitle': 'セットアップ手順',
          'admin.aiKickSetupStep1': 'ランナーを追加して、トークンと署名用の秘密を控える',
          'admin.aiKickSetupStep2': 'ランナーのマシン(Docker と Node.js 20 以上が必要)で、tools/ai-runner/README.md の手順に従って設定する(環境変数 SENN_URL、RUNNER_TOKEN、RUNNER_SIGNING_SECRET、CLAUDE_CODE_OAUTH_TOKEN、ALLOWED_REPOS を設定して node tools/ai-runner/src/runner.mjs)',
          'admin.aiKickSetupStep3': 'このページの「ランナー」が「接続中」になったことを確認し、プロジェクトの設定で AI 自動実行を許可する',
          'admin.aiKickSetupNote': 'Anthropic の API キー(従量課金)は使いません。ランナーは、API キーが設定されていると起動しません',
          'admin.aiKickSaved': '設定が保存されました',
          'admin.aiKickSaveFailed': '保存に失敗しました',
          'admin.aiKickNoRunners': 'ランナーはまだ作成されていません',
          'admin.aiKickRunnerNameDuplicate': 'この名前のランナーはすでに存在します',
          'admin.aiKickEnableConfirm': 'AI 自動実行を有効にしますか？',
          'common.cancel': 'キャンセル',
          'common.close': '閉じる',
          'common.copied': 'コピーしました',
          'common.save': '保存',
          'common.loading': '読み込み中...',
          'common.saving': '保存中...',
        },
      },
    },
    react: {
      useSuspense: false,
    },
  });
}

const mockAiKickData = {
  enabled: false,
  killed: false,
  limits: null,
  defaultLimits: {
    maxTurns: 5,
    maxMinutes: 30,
    maxWeightedTokens: 500_000,
    dailyBudgetSystem: 5_000_000,
    dailyBudgetProject: 1_000_000,
    weeklyBudgetSystem: 20_000_000,
    perUserHour: 10,
    perUserDay: 50,
    perTicket24h: 5,
    maxQueue: 100,
    burnRatePerMin: 100_000,
    maxConsecutiveFailures: 3,
    hardCeilingPercent: 200,
    emergencyCapPercent: 200,
    overrideAdminOnly: false,
  },
  runners: [],
  usage: {
    dayUsed: 0,
    weekUsed: 0,
    dayBudget: null,
    weekBudget: null,
    dayPercent: null,
    weekPercent: null,
  },
};

function setupTest() {
  const queryClient = new QueryClient();
  const mockSuccessToast = vi.fn();
  const mockErrorToast = vi.fn();

  mockToast.mockReturnValue({
    success: mockSuccessToast,
    error: mockErrorToast,
  });

  mockApiClient.get.mockResolvedValue({ data: mockAiKickData });
  mockApiClient.put.mockResolvedValue({ data: mockAiKickData });
  mockApiClient.post.mockResolvedValue({
    data: {
      id: 'runner-1',
      name: 'test-runner',
      token: 'senn_runner_test123',
      signingSecret: 'secret123',
      notice: 'Runner created',
    },
  });

  return {
    queryClient,
    mockSuccessToast,
    mockErrorToast,
  };
}

function renderComponent(queryClient: QueryClient) {
  return render(
    <QueryClientProvider client={queryClient}>
      <I18nextProvider i18n={i18n}>
        <AdminAiKickSection />
      </I18nextProvider>
    </QueryClientProvider>
  );
}

describe('AdminAiKickSection', () => {
  afterEach(() => cleanup());

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the section', async () => {
    const { queryClient } = setupTest();
    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByTestId('admin-ai-kick-section')).toBeInTheDocument();
    });
  });

  it('disables enable toggle when limits not saved', async () => {
    const { queryClient } = setupTest();
    renderComponent(queryClient);

    await waitFor(() => {
      const enableInput = screen.getByRole('checkbox', {
        name: /有効にする/i,
      }) as HTMLInputElement;
      expect(enableInput.disabled).toBe(true);
    });
  });

  it('shows emergency stop button when not killed', async () => {
    const { queryClient } = setupTest();
    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /緊急停止する/ })).toBeInTheDocument();
    });
  });

  it('sends emergency stop mutation with confirmation', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();
    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /緊急停止する/ })).toBeInTheDocument();
    });

    // Mock PUT response
    mockApiClient.put.mockResolvedValue({
      data: { ...mockAiKickData, killed: true },
    });

    const emergencyBtn = screen.getByRole('button', { name: /緊急停止する/ });
    await user.click(emergencyBtn);

    // Confirmation should appear
    await waitFor(() => {
      expect(screen.getByText(/すべての AI 自動実行を止めます/i)).toBeInTheDocument();
    });

    // Click confirm
    const confirmBtns = screen.getAllByRole('button', { name: /緊急停止する/ });
    const confirmBtn = confirmBtns[1];
    if (confirmBtn instanceof HTMLElement) {
      await user.click(confirmBtn);
    }

    await waitFor(() => {
      expect(mockApiClient.put).toHaveBeenCalledWith(
        '/system-admin/ai-kick/',
        { killed: true }
      );
    });
  });

  it('shows runner creation modal with token display', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();

    mockApiClient.get.mockResolvedValue({
      data: { ...mockAiKickData, runners: [] },
    });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /ランナーを追加/ })).toBeInTheDocument();
    });

    // Fill in runner name
    const runnerNameInput = screen.getByPlaceholderText('my-runner');
    await user.type(runnerNameInput, 'test-runner');

    // Add runner
    const addBtn = screen.getByRole('button', { name: /ランナーを追加/ });
    await user.click(addBtn);

    await waitFor(() => {
      expect(mockApiClient.post).toHaveBeenCalledWith(
        '/system-admin/ai-kick/runners/',
        { name: 'test-runner' }
      );
    });

    // Modal should show token
    const tokenInput = await screen.findByDisplayValue(/senn_runner_test123/);
    expect(tokenInput).toBeInTheDocument();
  });

  it('prevents modal close until checkbox checked', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();

    mockApiClient.get.mockResolvedValue({
      data: { ...mockAiKickData, runners: [] },
    });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /ランナーを追加/ })).toBeInTheDocument();
    });

    const runnerNameInput = screen.getByPlaceholderText('my-runner');
    await user.type(runnerNameInput, 'test-runner');

    const addBtn = screen.getByRole('button', { name: /ランナーを追加/ });
    await user.click(addBtn);

    const tokenInput = await screen.findByDisplayValue(/senn_runner_test123/);
    expect(tokenInput).toBeInTheDocument();

    // Close button should be disabled
    const closeBtn = screen.getByRole('button', { name: /閉じる/ });
    expect((closeBtn as HTMLButtonElement).disabled).toBe(true);

    // Check the saved checkbox
    const savedCheckbox = screen.getByLabelText(/保管しました/);
    await user.click(savedCheckbox);

    // Now close button should be enabled
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /閉じる/ })).toBeEnabled();
    });
  });

  it('does not show token in runner list', async () => {
    const { queryClient } = setupTest();

    mockApiClient.get.mockResolvedValue({
      data: {
        ...mockAiKickData,
        runners: [
          {
            id: 'runner-1',
            name: 'test-runner',
            tokenPrefix: 'senn_...abc',
            createdAt: '2024-01-01T00:00:00Z',
            revokedAt: null,
            lastSeenAt: '2024-01-01T12:00:00Z',
            online: true,
          },
        ],
      },
    });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByText('test-runner')).toBeInTheDocument();
    });

    // Only token prefix should be shown
    expect(screen.getByText(/senn_\.\.\.abc/)).toBeInTheDocument();
    // Full token should NOT be visible in runner list
    const tokenElement = screen.queryByDisplayValue(/senn_runner_test123/);
    expect(tokenElement).not.toBeInTheDocument();
  });

  it('displays 400 error from server', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();

    mockApiClient.get.mockResolvedValue({ data: mockAiKickData });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getByText(/初期値を入れる/i)).toBeInTheDocument();
    });

    // Mock error response
    mockApiClient.put.mockRejectedValue({
      response: {
        data: { detail: '日次予算はシステム全体より小さい必要があります' },
      },
    });

    // Try to save limits
    const defaultBtn = screen.getByText(/初期値を入れる/i);
    await user.click(defaultBtn);

    await waitFor(() => {
      expect(screen.getAllByDisplayValue('5').length).toBeGreaterThan(0);
    });

    const saveBtn = screen.getByRole('button', { name: /^保存$/ });
    await user.click(saveBtn);

    // Error should be displayed
    await waitFor(() => {
      expect(screen.getByText(/日次予算はシステム全体より小さい必要があります/)).toBeInTheDocument();
    });
  });

  it('上限が未保存のときは、初期値のまま保存できる(「初期値を入れる」を押さなくてよい)', async () => {
    const { queryClient } = setupTest();
    const user = userEvent.setup();

    mockApiClient.get.mockResolvedValue({ data: mockAiKickData });

    renderComponent(queryClient);

    const saveBtn = await screen.findByRole('button', { name: /^保存$/ });
    await waitFor(() => expect(saveBtn).toBeEnabled());

    await user.click(saveBtn);

    await waitFor(() => {
      expect(mockApiClient.put).toHaveBeenCalledWith('/system-admin/ai-kick/', {
        limits: mockAiKickData.defaultLimits,
      });
    });
  });

  it('上限が保存済みで、変更がないときは、保存できない', async () => {
    const { queryClient } = setupTest();

    mockApiClient.get.mockResolvedValue({
      data: { ...mockAiKickData, limits: mockAiKickData.defaultLimits },
    });

    renderComponent(queryClient);

    const saveBtn = await screen.findByRole('button', { name: /^保存$/ });
    expect(saveBtn).toBeDisabled();
  });

  it('大きい数字には、桁区切りの表示が付く', async () => {
    const { queryClient } = setupTest();

    mockApiClient.get.mockResolvedValue({ data: mockAiKickData });

    renderComponent(queryClient);

    await waitFor(() => {
      expect(screen.getAllByText(/\(5,000,000\)/).length).toBeGreaterThan(0);
    });
  });
});
