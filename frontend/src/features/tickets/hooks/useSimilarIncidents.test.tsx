// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

vi.mock('../api/similarIncidentsApi', () => ({
  fetchSimilarIncidents: vi.fn(),
}));

import { fetchSimilarIncidents } from '../api/similarIncidentsApi';
import { useSimilarIncidents, SIMILAR_DEBOUNCE_MS } from './useSimilarIncidents';

const mockFetch = vi.mocked(fetchSimilarIncidents);
const incident = {
  ticketKey: 'DEMO-000001',
  title: 't',
  score: 2,
  reasons: [],
  symptom: null,
  cause: null,
  solution: null,
};

describe('useSimilarIncidents', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockFetch.mockReset();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const args = (over: Partial<Parameters<typeof useSimilarIncidents>[0]> = {}) => ({
    enabled: true,
    projectPrefix: 'DEMO',
    title: '請求書の出力が失敗する',
    description: '',
    ...over,
  });

  it('入力が止まって待ち時間が過ぎてから、1 回だけ問い合わせる', async () => {
    mockFetch.mockResolvedValue({ engine: 'rules', incidents: [incident] });
    const { result, rerender } = renderHook((p) => useSimilarIncidents(p), {
      initialProps: args({ title: '請求書の出力' }),
    });
    rerender(args({ title: '請求書の出力が失' }));
    rerender(args({ title: '請求書の出力が失敗する' }));
    expect(mockFetch).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SIMILAR_DEBOUNCE_MS + 10);
    });
    expect(mockFetch).toHaveBeenCalledTimes(1);
    expect(mockFetch.mock.calls[0]?.[0].title).toBe('請求書の出力が失敗する');
    expect(result.current).toEqual({ incidents: [incident], searched: true });
  });

  it('無効・プロジェクト無し・短いタイトルでは問い合わせない', async () => {
    for (const p of [
      args({ enabled: false }),
      args({ projectPrefix: null }),
      args({ title: '短い' }),
      args({ title: '   ' }),
    ]) {
      renderHook(() => useSimilarIncidents(p));
    }
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SIMILAR_DEBOUNCE_MS * 3);
    });
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it('入力が変わったら、古い問い合わせの結果は捨てる', async () => {
    let resolveFirst: (v: Awaited<ReturnType<typeof fetchSimilarIncidents>>) => void = () => {};
    mockFetch.mockImplementationOnce(
      () =>
        new Promise<Awaited<ReturnType<typeof fetchSimilarIncidents>>>((r) => {
          resolveFirst = r;
        }),
    );
    mockFetch.mockResolvedValueOnce(null);
    const { result, rerender } = renderHook((p) => useSimilarIncidents(p), {
      initialProps: args(),
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SIMILAR_DEBOUNCE_MS + 10);
    });
    rerender(args({ title: '別の障害の題名です' }));
    await act(async () => {
      resolveFirst({ engine: 'rules', incidents: [incident] });
      await vi.advanceTimersByTimeAsync(SIMILAR_DEBOUNCE_MS + 10);
    });
    expect(result.current).toEqual({ incidents: [], searched: false });
  });

  it('無効に戻したら、表示を空にする', async () => {
    mockFetch.mockResolvedValue({ engine: 'rules', incidents: [incident] });
    const { result, rerender } = renderHook((p) => useSimilarIncidents(p), {
      initialProps: args(),
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SIMILAR_DEBOUNCE_MS + 10);
    });
    expect(result.current.incidents).toHaveLength(1);
    rerender(args({ enabled: false }));
    expect(result.current).toEqual({ incidents: [], searched: false });
  });

  it('0 件のときは searched=true、off・失敗(null)のときは searched=false', async () => {
    mockFetch.mockResolvedValueOnce({ engine: 'rules', incidents: [] });
    const a = renderHook(() => useSimilarIncidents(args()));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SIMILAR_DEBOUNCE_MS + 10);
    });
    expect(a.result.current).toEqual({ incidents: [], searched: true });

    mockFetch.mockResolvedValueOnce(null);
    const b = renderHook(() => useSimilarIncidents(args({ title: 'もう一つの障害の題名' })));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SIMILAR_DEBOUNCE_MS + 10);
    });
    expect(b.result.current).toEqual({ incidents: [], searched: false });
  });
});
