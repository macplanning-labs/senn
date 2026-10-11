// @vitest-environment jsdom
/**
 * CloseCheckDialog.test.tsx — クローズ確認ダイアログテスト
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { CloseCheckDialog } from './CloseCheckDialog';
import type { CloseCheckResponse } from '@/shared/api/types';

// i18n 設定
const initI18n = () => {
  if (i18n.isInitialized) return;
  i18n.init({
    lng: 'en',
    fallbackLng: 'en',
    resources: {
      en: {
        translation: {
          closeCheck: {
            title: 'Confirm Close',
            description: 'Before closing, please review these items:',
            cancelButton: 'Go Back',
            confirmButton: 'Close Anyway',
            blocker: {
              open_subtask: 'Incomplete subtasks: {{arg}}',
              no_assignee: 'No assignee set',
            },
          },
        },
      },
      ja: {
        translation: {
          closeCheck: {
            title: 'クローズ前の確認',
            description: 'クローズする前に以下の項目をご確認ください:',
            cancelButton: '戻って修正する',
            confirmButton: 'それでも完了にする',
            blocker: {
              open_subtask: '未完了のサブチケット: {{arg}}',
              no_assignee: '担当者が未設定です',
            },
          },
        },
      },
    },
  });
};

describe('CloseCheckDialog', () => {
  beforeEach(() => {
    initI18n();
  });

  afterEach(() => {
    cleanup();
  });

  const renderDialog = (props: any) => {
    return render(
      <I18nextProvider i18n={i18n}>
        <CloseCheckDialog {...props} />
      </I18nextProvider>
    );
  };

  it('should not render when isOpen is false', () => {
    const closeCheckResult: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [],
    };
    const { container } = renderDialog({
      isOpen: false,
      closeCheckResult,
      onConfirm: vi.fn(),
      onCancel: vi.fn(),
    });

    expect(container.querySelector('h3')).toBeNull();
  });

  it('should render dialog with title and description', () => {
    const closeCheckResult: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [
        { code: 'open_subtask', arg: 'SUBTASK-1', message: 'Open subtask' },
      ],
    };
    renderDialog({
      isOpen: true,
      closeCheckResult,
      onConfirm: vi.fn(),
      onCancel: vi.fn(),
    });

    expect(screen.getByText(/Confirm Close/)).toBeInTheDocument();
    expect(screen.getByText(/Before closing/)).toBeInTheDocument();
  });

  it('should render blockers with translated message', async () => {
    const closeCheckResult: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [
        { code: 'open_subtask', arg: 'SUBTASK-1', message: 'Open subtask' },
      ],
    };
    renderDialog({
      isOpen: true,
      closeCheckResult,
      onConfirm: vi.fn(),
      onCancel: vi.fn(),
    });

    await waitFor(() => {
      expect(screen.getByText(/Incomplete subtasks: SUBTASK-1/)).toBeInTheDocument();
    });
  });

  it('should render default message when blocker code is unknown', () => {
    const closeCheckResult: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [
        { code: 'unknown_code', arg: 'arg1', message: 'This is a custom message' },
      ],
    };
    renderDialog({
      isOpen: true,
      closeCheckResult,
      onConfirm: vi.fn(),
      onCancel: vi.fn(),
    });

    expect(screen.getByText(/This is a custom message/)).toBeInTheDocument();
  });

  it('should call onConfirm when confirm button is clicked', () => {
    const onConfirmMock = vi.fn();
    const closeCheckResult: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [
        { code: 'no_assignee', arg: '', message: 'No assignee' },
      ],
    };
    renderDialog({
      isOpen: true,
      closeCheckResult,
      onConfirm: onConfirmMock,
      onCancel: vi.fn(),
    });

    const confirmButton = screen.getByText(/Close Anyway/);
    fireEvent.click(confirmButton);

    expect(onConfirmMock).toHaveBeenCalledOnce();
  });

  it('should call onCancel when cancel button is clicked', () => {
    const onCancelMock = vi.fn();
    const closeCheckResult: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [
        { code: 'no_assignee', arg: '', message: 'No assignee' },
      ],
    };
    renderDialog({
      isOpen: true,
      closeCheckResult,
      onConfirm: vi.fn(),
      onCancel: onCancelMock,
    });

    const cancelButton = screen.getByText(/Go Back/);
    fireEvent.click(cancelButton);

    expect(onCancelMock).toHaveBeenCalledOnce();
  });

  it('should call onCancel when overlay is clicked', () => {
    const onCancelMock = vi.fn();
    const closeCheckResult: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [],
    };
    const { container } = renderDialog({
      isOpen: true,
      closeCheckResult,
      onConfirm: vi.fn(),
      onCancel: onCancelMock,
    });

    const overlay = container.querySelector('[style*="position: fixed"]');
    if (overlay) {
      fireEvent.click(overlay);
    }

    expect(onCancelMock).toHaveBeenCalledOnce();
  });

  it('should render multiple blockers', () => {
    const closeCheckResult: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [
        { code: 'open_subtask', arg: 'SUBTASK-1', message: 'Open subtask' },
        { code: 'no_assignee', arg: '', message: 'No assignee' },
      ],
    };
    renderDialog({
      isOpen: true,
      closeCheckResult,
      onConfirm: vi.fn(),
      onCancel: vi.fn(),
    });

    const blockers = screen.getAllByText(/No assignee/);
    expect(blockers.length).toBeGreaterThan(0);
  });
});
