// @vitest-environment jsdom
/**
 * AiKickDialog.test.tsx - AI Kick ダイアログのテスト
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { AiKickDialog } from './AiKickDialog';
import * as api from '../api';

// モック設定（i18n.ts のインポートをモック）
vi.mock('../i18n', () => ({}));
vi.mock('@/shared/stores/toastStore');

// API のモック
vi.mock('../api', async () => {
  const actual = await vi.importActual('../api');
  return {
    ...actual,
    getAiKickPreview: vi.fn(),
    kickAi: vi.fn(),
    parseAiKickError: (error: any) => ({
      status: error?.response?.status || 0,
      code: error?.response?.data?.code,
      detail: error?.response?.data?.detail,
      warnings: error?.response?.data?.warnings,
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
          'aiKick.dialog.title': 'AIに対応させる',
          'aiKick.dialog.preparing': 'AIに渡す内容を準備しています…',
          'aiKick.dialog.close': '閉じる',
          'aiKick.dialog.cancel': 'キャンセル',
          'aiKick.dialog.execute': '実行',
          'aiKick.dialog.retry': 'もう一度',
          'aiKick.dialog.note': 'AI は変更とコミットまで行います',
          'aiKick.dialog.dataSharing': 'このチケットの内容と、リポジトリ「{{repo}}」のコードが Anthropic 社に送られます',
          'aiKick.dialog.repository': 'リポジトリ',
          'aiKick.dialog.branch': 'ブランチ',
          'aiKick.dialog.limits.title': '実行の上限',
          'aiKick.dialog.limits.turns': 'ターン',
          'aiKick.dialog.limits.minutes': '分',
          'aiKick.dialog.limits.tokens': 'トークン',
          'aiKick.dialog.preview.title': 'AI に渡る内容を確認する',
          'aiKick.dialog.confirmation.default': '警告の内容を確認し、承知して実行します',
          'aiKick.dialog.confirmation.emergency': '緊急として実行します',
          'aiKick.dialog.denied': '実行できません',
          'aiKick.dialog.error.promptChanged': '確認したあとに、チケットの内容が変わりました',
          'aiKick.dialog.error.needsConfirm': '警告内容を確認してください',
          'aiKick.dialog.error.generic': 'エラーが発生しました',
          'aiKick.dialog.success': 'AI の順番待ちに入りました',
          'aiKick.dialog.warning.blocksOpen': 'このチケットが終わらないと進められる作業が {{count}} 件',
          'aiKick.dialog.warning.externalContent': 'ゲストが書いた内容を含みます',
          'aiKick.priority.normal': '通常',
          'aiKick.priority.emergency': '緊急',
        },
      },
    },
  });
}

const testI18n = i18n.cloneInstance();

function makePreview(overrides = {}): api.AiKickPreview {
  return {
    verdict: 'allow',
    warnings: [],
    ticketKey: 'PROJ-123',
    prompt: 'Test prompt content',
    promptHash: 'test-hash',
    priority: 1,
    priorityClass: 'normal',
    priorityLabel: 'Normal',
    blocksOpen: 0,
    caps: {
      maxTurns: 10,
      maxMinutes: 30,
      maxWeightedTokens: 100000,
    },
    repo: 'test-repo',
    baseBranch: 'develop',
    externalContent: false,
    ...overrides,
  };
}

describe('AiKickDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it('needs_confirm のときチェックするまで実行ボタンが無効', async () => {
    const preview = makePreview({ verdict: 'needs_confirm' });
    vi.mocked(api.getAiKickPreview).mockResolvedValue(preview);

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={vi.fn()}
        />
      </I18nextProvider>
    );

    await waitFor(() => {
      const btn = screen.getByTestId('ai-kick-execute-btn') as HTMLButtonElement;
      expect(btn.disabled).toBe(true);
    });

    // チェックボックスをチェック
    const checkbox = screen.getByRole('checkbox') as HTMLInputElement;
    await userEvent.click(checkbox);

    // ボタンが有効になることを確認
    await waitFor(() => {
      const btn = screen.getByTestId('ai-kick-execute-btn') as HTMLButtonElement;
      expect(btn.disabled).toBe(false);
    });
  });

  it('needs_confirm のときは confirmed: true で送る', async () => {
    const preview = makePreview({ verdict: 'needs_confirm' });
    vi.mocked(api.getAiKickPreview).mockResolvedValue(preview);
    vi.mocked(api.kickAi).mockResolvedValue({
      runUuid: 'uuid-1',
      status: 'queued',
      priorityClass: 'normal',
    });

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={vi.fn()}
          onRunCreated={vi.fn()}
        />
      </I18nextProvider>
    );

    // チェック
    const checkbox = await screen.findByRole('checkbox') as HTMLInputElement;
    await userEvent.click(checkbox);

    // 実行
    const btn = screen.getByTestId('ai-kick-execute-btn');
    await userEvent.click(btn);

    await waitFor(() => {
      expect(vi.mocked(api.kickAi)).toHaveBeenCalledWith('PROJ-123', {
        promptHash: 'test-hash',
        confirmed: true,
      });
    });
  });

  it('deny のときは実行ボタンがない', async () => {
    const preview = makePreview({
      verdict: 'deny',
      denied: { code: 'not_allowed', message: 'Not allowed' },
    });
    vi.mocked(api.getAiKickPreview).mockResolvedValue(preview);

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={vi.fn()}
        />
      </I18nextProvider>
    );

    await waitFor(() => {
      expect(screen.queryByTestId('ai-kick-execute-btn')).not.toBeInTheDocument();
    });
  });

  it('allow で警告なしなら confirmed: false で送る', async () => {
    const preview = makePreview({ verdict: 'allow', warnings: [] });
    vi.mocked(api.getAiKickPreview).mockResolvedValue(preview);
    vi.mocked(api.kickAi).mockResolvedValue({
      runUuid: 'uuid-1',
      status: 'queued',
      priorityClass: 'normal',
    });

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={vi.fn()}
          onRunCreated={vi.fn()}
        />
      </I18nextProvider>
    );

    await waitFor(() => {
      const btn = screen.getByTestId('ai-kick-execute-btn') as HTMLButtonElement;
      expect(btn.disabled).toBe(false);
    });

    // チェックボックスがない
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();

    // 実行ボタンをクリック
    const btn = screen.getByTestId('ai-kick-execute-btn');
    await userEvent.click(btn);

    // confirmed: false で送られる
    await waitFor(() => {
      expect(vi.mocked(api.kickAi)).toHaveBeenCalledWith('PROJ-123', {
        promptHash: 'test-hash',
        confirmed: false,
      });
    });
  });

  it('409 prompt_changed でプレビューを取り直す', async () => {
    const preview = makePreview({ verdict: 'needs_confirm' });
    const updatedPreview = makePreview({
      verdict: 'allow',
      promptHash: 'new-hash',
    });

    vi.mocked(api.getAiKickPreview)
      .mockResolvedValueOnce(preview)
      .mockResolvedValueOnce(updatedPreview);

    const error = new Error('prompt_changed') as any;
    error.response = {
      status: 409,
      data: { code: 'prompt_changed', detail: 'チケットの内容が変わりました' },
    };
    vi.mocked(api.kickAi).mockRejectedValueOnce(error);

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={vi.fn()}
        />
      </I18nextProvider>
    );

    // 最初のプレビュー確認
    const checkbox = await screen.findByRole('checkbox') as HTMLInputElement;
    await userEvent.click(checkbox);

    // 実行
    const btn = screen.getByTestId('ai-kick-execute-btn');
    await userEvent.click(btn);

    // エラーメッセージが表示され、チェックが外れる
    await waitFor(() => {
      expect(checkbox.checked).toBe(false);
    });
  });

  it('送信中の二重クリックで 1 回しか送らない', async () => {
    const preview = makePreview({ verdict: 'allow' });
    vi.mocked(api.getAiKickPreview).mockResolvedValue(preview);

    let resolveKickAi: any;
    vi.mocked(api.kickAi).mockReturnValue(
      new Promise((resolve) => {
        resolveKickAi = resolve;
      })
    );

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={vi.fn()}
        />
      </I18nextProvider>
    );

    const btn = await screen.findByTestId('ai-kick-execute-btn');

    // 二重クリック
    fireEvent.click(btn);
    fireEvent.click(btn);

    // 1 回だけ送られる
    resolveKickAi({
      runUuid: 'uuid-1',
      status: 'queued',
      priorityClass: 'normal',
    });

    await waitFor(() => {
      expect(vi.mocked(api.kickAi)).toHaveBeenCalledTimes(1);
    });
  });

  it('externalContent の警告が表示される', async () => {
    const preview = makePreview({ externalContent: true });
    vi.mocked(api.getAiKickPreview).mockResolvedValue(preview);

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={vi.fn()}
        />
      </I18nextProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/ゲスト|guest/i)).toBeInTheDocument();
    });
  });

  it('実行の前に、チケットの内容とリポジトリのコードが Anthropic 社に送られることを表示する', async () => {
    vi.mocked(api.getAiKickPreview).mockResolvedValue(makePreview());

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={vi.fn()}
        />
      </I18nextProvider>
    );

    const note = await screen.findByTestId('ai-kick-data-sharing');
    expect(note.textContent).toMatch(/Anthropic/);
    expect(note.textContent).toMatch(/test-repo/);
  });

  it('Esc キーで閉じられる', async () => {
    const onClose = vi.fn();
    const preview = makePreview();
    vi.mocked(api.getAiKickPreview).mockResolvedValue(preview);

    render(
      <I18nextProvider i18n={testI18n}>
        <AiKickDialog
          ticketKey="PROJ-123"
          onClose={onClose}
        />
      </I18nextProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('ai-kick-dialog')).toBeInTheDocument();
    });

    fireEvent.keyDown(document, { key: 'Escape' });

    expect(onClose).toHaveBeenCalled();
  });
});
