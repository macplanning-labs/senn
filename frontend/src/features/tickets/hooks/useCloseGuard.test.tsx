// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import type { CloseCheckResponse } from '@/shared/api/types';

vi.mock('../api/closeCheckApi', () => ({
  getCloseWarning: vi.fn(),
}));

import { getCloseWarning } from '../api/closeCheckApi';
import { useCloseGuard } from './useCloseGuard';

const mockWarning = vi.mocked(getCloseWarning);

const warning: CloseCheckResponse = {
  engine: 'rules',
  canClose: false,
  checkedAt: '2026-10-08T00:00:00Z',
  blockers: [{ code: 'open_subtask', arg: 'DEMO-000002', message: 'x' }],
};

describe('useCloseGuard', () => {
  beforeEach(() => {
    mockWarning.mockReset();
  });

  it('完了以外のステータスは確認せず即座に実行する', async () => {
    const onProceed = vi.fn();
    const { result } = renderHook(() => useCloseGuard('DEMO-000001', onProceed));
    await act(async () => {
      await result.current.request('resolved');
      await result.current.request('in_progress');
    });
    expect(mockWarning).not.toHaveBeenCalled();
    expect(onProceed.mock.calls).toEqual([['resolved'], ['in_progress']]);
    expect(result.current.pending).toBeNull();
  });

  it('完了で警告が無ければ、そのまま1回だけ実行する', async () => {
    mockWarning.mockResolvedValue(null);
    const onProceed = vi.fn();
    const { result } = renderHook(() => useCloseGuard('DEMO-000001', onProceed));
    await act(async () => {
      await result.current.request('closed');
    });
    expect(mockWarning).toHaveBeenCalledWith('DEMO-000001');
    expect(onProceed).toHaveBeenCalledTimes(1);
    expect(onProceed).toHaveBeenCalledWith('closed');
    expect(result.current.pending).toBeNull();
  });

  it('完了で警告があれば、実行せず保留してダイアログ用の結果を持つ', async () => {
    mockWarning.mockResolvedValue(warning);
    const onProceed = vi.fn();
    const { result } = renderHook(() => useCloseGuard('DEMO-000001', onProceed));
    await act(async () => {
      await result.current.request('closed');
    });
    expect(onProceed).not.toHaveBeenCalled();
    expect(result.current.pending?.status).toBe('closed');
    expect(result.current.pending?.result.blockers).toHaveLength(1);
  });

  it('「それでも完了にする」で1回だけ実行して保留を解除する', async () => {
    mockWarning.mockResolvedValue(warning);
    const onProceed = vi.fn();
    const { result } = renderHook(() => useCloseGuard('DEMO-000001', onProceed));
    await act(async () => {
      await result.current.request('closed');
    });
    act(() => result.current.confirm());
    expect(onProceed).toHaveBeenCalledTimes(1);
    expect(onProceed).toHaveBeenCalledWith('closed');
    expect(result.current.pending).toBeNull();
    // 連打しても2回目は実行されない
    act(() => result.current.confirm());
    expect(onProceed).toHaveBeenCalledTimes(1);
  });

  it('「戻って修正する」では実行しない', async () => {
    mockWarning.mockResolvedValue(warning);
    const onProceed = vi.fn();
    const { result } = renderHook(() => useCloseGuard('DEMO-000001', onProceed));
    await act(async () => {
      await result.current.request('closed');
    });
    act(() => result.current.dismiss());
    expect(onProceed).not.toHaveBeenCalled();
    expect(result.current.pending).toBeNull();
  });

  it('確認中の連打では確認も実行も1回だけ', async () => {
    let release: (v: CloseCheckResponse | null) => void = () => {};
    mockWarning.mockImplementation(
      () => new Promise<CloseCheckResponse | null>((resolve) => (release = resolve)),
    );
    const onProceed = vi.fn();
    const { result } = renderHook(() => useCloseGuard('DEMO-000001', onProceed));
    let first: Promise<void> = Promise.resolve();
    act(() => {
      first = result.current.request('closed');
      void result.current.request('closed');
      void result.current.request('closed');
    });
    expect(mockWarning).toHaveBeenCalledTimes(1);
    await act(async () => {
      release(null);
      await first;
    });
    await waitFor(() => expect(onProceed).toHaveBeenCalledTimes(1));
  });

  it('警告ダイアログの表示中は、新しい完了操作を受け付けない', async () => {
    mockWarning.mockResolvedValue(warning);
    const onProceed = vi.fn();
    const { result } = renderHook(() => useCloseGuard('DEMO-000001', onProceed));
    await act(async () => {
      await result.current.request('closed');
    });
    await act(async () => {
      await result.current.request('closed');
    });
    expect(mockWarning).toHaveBeenCalledTimes(1);
  });

  it('チケットキーが無いときは確認結果が null で、そのまま実行する', async () => {
    mockWarning.mockResolvedValue(null);
    const onProceed = vi.fn();
    const { result } = renderHook(() => useCloseGuard(undefined, onProceed));
    await act(async () => {
      await result.current.request('closed');
    });
    expect(onProceed).toHaveBeenCalledWith('closed');
  });
});
