// @vitest-environment jsdom
/**
 * AiKickStatus.test.tsx - AI Kick ステータスコンポーネントのテスト
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { AiKickStatus } from './AiKickStatus';
import * as api from '../api';

// モック設定（i18n.ts のインポートをモック）
vi.mock('../i18n', () => ({}));
vi.mock('@/shared/stores/toastStore');

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
          'aiKick.statusPanel.title': '最新の AI 実行',
          'aiKick.statusPanel.history': '履歴を見る',
          'aiKick.statusPanel.cancel': '取り消す',
          'aiKick.statusPanel.cancelConfirm': '実行を取り消しますか?',
          'aiKick.statusPanel.cancelSuccess': '実行を取り消しました',
          'aiKick.statusPanel.cancelError': '取り消せるのは起動した人と管理者だけです',
          'aiKick.statusPanel.summary': '要約',
          'aiKick.statusPanel.error': 'エラー詳細',
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

describe('AiKickStatus', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it('実行が無ければ何も表示しない', async () => {
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs: [] });

    const { container } = render(
      <I18nextProvider i18n={testI18n}>
        <AiKickStatus ticketKey="PROJ-123" />
      </I18nextProvider>
    );

    await waitFor(() => {
      expect(container.querySelector('[data-testid="ai-kick-status"]')).not.toBeInTheDocument();
    });
  });

  it('状態ごとの表示', async () => {
    const runs = [
      makeRun({ runUuid: 'uuid-1', status: 'finished', result: 'resolved' }),
    ];
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs });

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickStatus ticketKey="PROJ-123" />
      </I18nextProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('ai-kick-status')).toBeInTheDocument();
    });
  });

  it('アクティブな実行があるときだけ取り消しボタンが出る', async () => {
    const activeRun = makeRun({ status: 'running' });
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs: [activeRun] });

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickStatus ticketKey="PROJ-123" />
      </I18nextProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('ai-kick-cancel-btn')).toBeInTheDocument();
    });
  });

  it('finished の実行は取り消しボタンが無い', async () => {
    const finishedRun = makeRun({ status: 'finished', result: 'resolved' });
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs: [finishedRun] });

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickStatus ticketKey="PROJ-123" />
      </I18nextProvider>
    );

    await waitFor(() => {
      expect(screen.queryByTestId('ai-kick-cancel-btn')).not.toBeInTheDocument();
    });
  });

  it('実行が無ければ何も出ない', async () => {
    vi.mocked(api.getAiKickRuns).mockResolvedValue({ runs: [] });

    const { container } = render(
      <I18nextProvider i18n={testI18n}>
        <AiKickStatus ticketKey="PROJ-123" />
      </I18nextProvider>
    );

    await waitFor(() => {
      const statusEl = container.querySelector('[data-testid="ai-kick-status"]');
      expect(statusEl).not.toBeInTheDocument();
    });
  });
});
