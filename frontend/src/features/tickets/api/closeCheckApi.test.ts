/**
 * closeCheckApi.test.ts — クローズ確認 API テスト
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { fetchCloseCheck, getCloseWarning, shouldWarnBeforeClose } from './closeCheckApi';
import * as client from '@/shared/api/client';
import type { CloseCheckResponse } from '@/shared/api/types';

vi.mock('@/shared/api/client', () => ({
  apiClient: {
    get: vi.fn(),
  },
}));

describe('closeCheckApi', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('fetchCloseCheck', () => {
    it('should return CloseCheckResponse on 200', async () => {
      const mockResponse: CloseCheckResponse = {
        engine: 'rules',
        canClose: true,
        checkedAt: '2026-10-08T12:00:00Z',
        blockers: [],
      };
      const mockApiClient = client.apiClient as any;
      mockApiClient.get.mockResolvedValue({ data: mockResponse });

      const result = await fetchCloseCheck('DEMO-000001');

      expect(result).toEqual(mockResponse);
      expect(mockApiClient.get).toHaveBeenCalledWith(
        '/tickets/DEMO-000001/close-check/',
        expect.any(Object)
      );
    });

    it('should return null on 503 error', async () => {
      const mockApiClient = client.apiClient as any;
      mockApiClient.get.mockRejectedValue(new Error('503 Service Unavailable'));

      const result = await fetchCloseCheck('DEMO-000002');

      expect(result).toBeNull();
    });

    it('should return null on network error', async () => {
      const mockApiClient = client.apiClient as any;
      mockApiClient.get.mockRejectedValue(new Error('Network error'));

      const result = await fetchCloseCheck('DEMO-000003');

      expect(result).toBeNull();
    });

    it('should return null on timeout', async () => {
      const mockApiClient = client.apiClient as any;
      mockApiClient.get.mockImplementation(
        () => new Promise((_, reject) => {
          const error = new Error('Timeout');
          (error as any).code = 'ECONNABORTED';
          reject(error);
        })
      );

      const result = await fetchCloseCheck('DEMO-000004');

      expect(result).toBeNull();
    });
  });

  describe('shouldWarnBeforeClose', () => {
    it('should return false when res is null', () => {
      const result = shouldWarnBeforeClose(null);
      expect(result).toBe(false);
    });

    it('should return false when engine is off', () => {
      const res: CloseCheckResponse = {
        engine: 'off',
        canClose: false,
        checkedAt: '2026-10-08T12:00:00Z',
        blockers: [],
      };
      const result = shouldWarnBeforeClose(res);
      expect(result).toBe(false);
    });

    it('should return false when canClose is true and blockers is empty', () => {
      const res: CloseCheckResponse = {
        engine: 'rules',
        canClose: true,
        checkedAt: '2026-10-08T12:00:00Z',
        blockers: [],
      };
      const result = shouldWarnBeforeClose(res);
      expect(result).toBe(false);
    });

    it('should return true when canClose is false and blockers has items', () => {
      const res: CloseCheckResponse = {
        engine: 'rules',
        canClose: false,
        checkedAt: '2026-10-08T12:00:00Z',
        blockers: [
          { code: 'open_subtask', arg: 'SUBTASK-1', message: 'Open subtask' },
        ],
      };
      const result = shouldWarnBeforeClose(res);
      expect(result).toBe(true);
    });

    it('should return false when canClose is null', () => {
      const res: CloseCheckResponse = {
        engine: 'rules',
        canClose: null,
        checkedAt: '2026-10-08T12:00:00Z',
        blockers: [],
      };
      const result = shouldWarnBeforeClose(res);
      expect(result).toBe(false);
    });

    it('should return true with multiple blockers', () => {
      const res: CloseCheckResponse = {
        engine: 'rules',
        canClose: false,
        checkedAt: '2026-10-08T12:00:00Z',
        blockers: [
          { code: 'open_subtask', arg: 'SUBTASK-1', message: 'Open subtask' },
          { code: 'no_assignee', arg: '', message: 'No assignee' },
        ],
      };
      const result = shouldWarnBeforeClose(res);
      expect(result).toBe(true);
    });
  });

  describe('getCloseWarning', () => {
    const blocked: CloseCheckResponse = {
      engine: 'rules',
      canClose: false,
      checkedAt: '2026-10-08T12:00:00Z',
      blockers: [{ code: 'open_subtask', arg: 'DEMO-000002', message: 'x' }],
    };

    it('警告が必要なときだけ結果を返す', async () => {
      (client.apiClient.get as ReturnType<typeof vi.fn>).mockResolvedValue({ data: blocked });
      expect(await getCloseWarning('DEMO-000001')).toEqual(blocked);
    });

    it('問題なし・無効・失敗のときは null(そのまま続行)', async () => {
      const get = client.apiClient.get as ReturnType<typeof vi.fn>;
      get.mockResolvedValue({
        data: { engine: 'rules', canClose: true, checkedAt: 'x', blockers: [] },
      });
      expect(await getCloseWarning('DEMO-000001')).toBeNull();
      get.mockResolvedValue({
        data: { engine: 'off', canClose: null, checkedAt: 'x', blockers: [] },
      });
      expect(await getCloseWarning('DEMO-000001')).toBeNull();
      get.mockRejectedValue(new Error('503'));
      expect(await getCloseWarning('DEMO-000001')).toBeNull();
    });

    it('キーが無いときは API を呼ばず null', async () => {
      const get = client.apiClient.get as ReturnType<typeof vi.fn>;
      get.mockClear();
      expect(await getCloseWarning(undefined)).toBeNull();
      expect(get).not.toHaveBeenCalled();
    });
  });
});
