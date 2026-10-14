// @vitest-environment jsdom
/**
 * AiKickHistoryPopup.test.tsx - AI 実行履歴ポップアップのテスト
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { AiKickHistoryPopup } from './AiKickHistoryPopup';
import * as api from '../api';

// モック設定（i18n.ts のインポートをモック）
vi.mock('../i18n', () => ({}));

const toastMock = { success: vi.fn(), error: vi.fn() };
vi.mock('@/shared/stores/toastStore', () => ({
  useToast: () => toastMock,
}));

// API のモック
vi.mock('../api', async () => {
  const actual = await vi.importActual('../api');
  return {
    ...actual,
    getAiKickRuns: vi.fn(),
    cancelAiKickRun: vi.fn(),
    parseAiKickError: (error: any) => ({
      status: error?.response?.status || 0,
      detail: error?.response?.data?.detail,
    }),
  };
});

// テスト用 i18n インスタンス初期化
if (!i18n.isInitialized) {
  void i18n.init({
    lng: 'ja',
    fallbackLng: 'ja',
    resources: {
      ja: {
        translation: {
          'aiKick.statusPanel.popupTitle': 'AI実行履歴',
          'aiKick.statusPanel.popupEmpty': '実行履歴はありません',
          'aiKick.statusPanel.popupClose': '閉じる',
          'aiKick.statusPanel.colWhen': '日時',
          'aiKick.statusPanel.colWho': '実行者',
          'aiKick.statusPanel.colStatus': '状態',
          'aiKick.statusPanel.colTokens': 'トークン',
          'aiKick.statusPanel.cancel': '取り消す',
          'aiKick.statusPanel.cancelConfirm': '実行を取り消しますか?',
          'aiKick.statusPanel.cancelSuccess': '実行を取り消しました',
          'aiKick.statusPanel.cancelError': '取り消せるのは起動した人と管理者だけです',
          'aiKick.status.queued': '順番待ち',
          'aiKick.status.running': '実行中',
          'aiKick.status.stopping': '停止中',
          'aiKick.status.finished': '完了',
          'aiKick.status.resolved': '完了',
          'aiKick.status.blocked': 'ブロック',
          'aiKick.status.aborted': '中止',
          'aiKick.status.error': 'エラー',
          'aiKick.result.resolved': '完了',
          'aiKick.result.blocked': 'ブロック',
          'aiKick.result.aborted': '中止',
          'aiKick.result.error': 'エラー',
          'aiKick.stopReason.unknown': '不明',
          'aiKick.priority.normal': '通常',
          'aiKick.priority.high': '高',
          'aiKick.priority.emergency': '緊急',
        },
      },
    },
  });
}

const testI18n = i18n.cloneInstance();

function makeRun(overrides: Partial<api.AiKickRun> = {}): api.AiKickRun {
  return {
    runUuid: 'uuid-1',
    status: 'queued',
    triggerType: 'manual',
    priorityClass: 'normal',
    queuedAt: '2026-10-07T12:00:00Z',
    startedAt: null,
    endedAt: null,
    result: null,
    summary: null,
    weightedTokens: 1000,
    stopReason: null,
    errorDetail: null,
    triggeredBy: null,
    ...overrides,
  };
}

function renderPopup(onClose = vi.fn()) {
  return render(
    <I18nextProvider i18n={testI18n}>
      <AiKickHistoryPopup ticketKey="PROJ-123" onClose={onClose} />
    </I18nextProvider>
  );
}

describe('AiKickHistoryPopup', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('実行が 2 件あるとき、日時・状態・トークンが行に出る', async () => {
    const runs = [
      makeRun({
        runUuid: 'uuid-new',
        status: 'finished',
        result: 'resolved',
        queuedAt: '2026-10-07T12:00:00Z',
        weightedTokens: 2500,
      }),
      makeRun({
        runUuid: 'uuid-old',
        status: 'queued',
        queuedAt: '2026-10-06T09:00:00Z',
        weightedTokens: 0,
      }),
    ];
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs });

    renderPopup();

    await waitFor(() => {
      expect(screen.getAllByTestId('ai-kick-history-row')).toHaveLength(2);
    });

    const rows = screen.getAllByTestId('ai-kick-history-row');
    expect(rows[0]).toHaveTextContent(new Date(runs[0]!.queuedAt).toLocaleString());
    expect(rows[0]).toHaveTextContent('完了');
    expect(rows[0]).toHaveTextContent('3k');
    expect(rows[1]).toHaveTextContent('順番待ち');
    expect(api.getAiKickRuns).toHaveBeenCalledWith('PROJ-123');
  });

  it('実行者の名前が行に出て、名前がなければ —(ダッシュ)を出す', async () => {
    const runs = [
      makeRun({
        runUuid: 'uuid-named',
        triggeredBy: { id: 7, name: '実行 太郎' },
      }),
      makeRun({ runUuid: 'uuid-anon', triggeredBy: null }),
    ];
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs });

    renderPopup();

    await waitFor(() => {
      expect(screen.getAllByTestId('ai-kick-history-row')).toHaveLength(2);
    });

    const whoCells = screen.getAllByTestId('ai-kick-history-who');
    expect(whoCells[0]).toHaveTextContent('実行 太郎');
    expect(whoCells[0]).toHaveAttribute('title', '実行 太郎');
    expect(whoCells[1]).toHaveTextContent('—');
  });

  it('実行が 0 件のとき「実行履歴はありません」を出す', async () => {
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs: [] });

    renderPopup();

    await waitFor(() => {
      expect(screen.getByText('実行履歴はありません')).toBeInTheDocument();
    });
    expect(screen.queryAllByTestId('ai-kick-history-row')).toHaveLength(0);
  });

  it('Esc キーで onClose が呼ばれる', async () => {
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs: [] });
    const onClose = vi.fn();

    renderPopup(onClose);

    await waitFor(() => {
      expect(screen.getByTestId('ai-kick-history-popup')).toBeInTheDocument();
    });

    fireEvent.keyDown(document, { key: 'Escape' });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('実行中の行にだけ取り消しボタンが出て、押すと取り消し API が呼ばれる', async () => {
    const runs = [
      makeRun({ runUuid: 'uuid-active', status: 'running' }),
      makeRun({ runUuid: 'uuid-done', status: 'finished', result: 'resolved' }),
    ];
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs });
    vi.mocked(api.cancelAiKickRun).mockResolvedValue({ status: 'stopping' });
    vi.stubGlobal('confirm', vi.fn(() => true));

    renderPopup();

    await waitFor(() => {
      expect(screen.getAllByTestId('ai-kick-cancel-btn')).toHaveLength(1);
    });

    fireEvent.click(screen.getByTestId('ai-kick-cancel-btn'));

    await waitFor(() => {
      expect(api.cancelAiKickRun).toHaveBeenCalledWith('PROJ-123', 'uuid-active');
    });
    expect(toastMock.success).toHaveBeenCalledWith('実行を取り消しました');
  });
});
