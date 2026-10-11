/**
 * similarIncidentsApi.ts — 新規バグ起票画面の「過去の類似障害」API
 *
 * 付加情報なので、失敗・タイムアウト・エンジン off のときは null を返し、画面には何も出さない。
 * 検索が実行されて 0 件だったときは、空の一覧を返す(画面が「見つからなかった」案内を出せるように)。
 */

import { apiClient } from '@/shared/api/client';
import type { SimilarIncidentsResponse } from '@/shared/api/types';

const SIMILAR_INCIDENTS_TIMEOUT_MS = 5000;

export interface SimilarIncidentsInput {
  projectPrefix: string;
  title: string;
  description: string;
}

/**
 * 過去の類似障害を取得する
 *
 * @returns rules で検索できたときの結果(0 件も含む)。off・失敗・タイムアウトは null
 */
export async function fetchSimilarIncidents(
  input: SimilarIncidentsInput,
): Promise<SimilarIncidentsResponse | null> {
  try {
    const response = await apiClient.post<SimilarIncidentsResponse>(
      '/ai/similar-incidents/',
      {
        projectPrefix: input.projectPrefix,
        title: input.title,
        description: input.description,
      },
      { timeout: SIMILAR_INCIDENTS_TIMEOUT_MS },
    );
    const data = response.data;
    if (data.engine !== 'rules' || !Array.isArray(data.incidents)) {
      return null;
    }
    return data;
  } catch {
    return null;
  }
}
