import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchSimilarIncidents } from './similarIncidentsApi';
import * as client from '@/shared/api/client';
import type { SimilarIncidentsResponse } from '@/shared/api/types';

vi.mock('@/shared/api/client', () => ({
  apiClient: { post: vi.fn() },
}));

const input = { projectPrefix: 'DEMO', title: '請求書の出力が失敗する', description: '' };
const incident = {
  ticketKey: 'DEMO-000001',
  title: '過去の障害',
  score: 2,
  reasons: ['共通する語: 請求書'],
  symptom: null,
  cause: null,
  solution: null,
};

describe('fetchSimilarIncidents', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('rules で 1 件以上あれば、そのまま返し、正しい本文で呼ぶ', async () => {
    const res: SimilarIncidentsResponse = { engine: 'rules', incidents: [incident] };
    vi.mocked(client.apiClient.post).mockResolvedValue({ data: res });
    expect(await fetchSimilarIncidents(input)).toEqual(res);
    expect(client.apiClient.post).toHaveBeenCalledWith(
      '/ai/similar-incidents/',
      { projectPrefix: 'DEMO', title: '請求書の出力が失敗する', description: '' },
      { timeout: 5000 },
    );
  });

  it('rules で 0 件なら、空の一覧を返す(「見つからなかった」案内を出せるように)', async () => {
    const res: SimilarIncidentsResponse = { engine: 'rules', incidents: [] };
    vi.mocked(client.apiClient.post).mockResolvedValue({ data: res });
    expect(await fetchSimilarIncidents(input)).toEqual(res);
  });

  it('off と失敗は null(画面に何も出さない)', async () => {
    vi.mocked(client.apiClient.post).mockResolvedValueOnce({ data: { engine: 'off', incidents: [] } });
    expect(await fetchSimilarIncidents(input)).toBeNull();
    vi.mocked(client.apiClient.post).mockRejectedValueOnce(new Error('503'));
    expect(await fetchSimilarIncidents(input)).toBeNull();
  });

  it('不正な形(incidents が配列でない)も null', async () => {
    vi.mocked(client.apiClient.post).mockResolvedValue({
      data: { engine: 'rules', incidents: 'x' },
    });
    expect(await fetchSimilarIncidents(input)).toBeNull();
  });
});
