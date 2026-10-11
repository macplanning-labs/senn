/**
 * useSimilarIncidents.ts — 入力中のタイトル・説明に似た過去の障害を、少し待ってから探す
 *
 * 入力のたびに呼ばず、入力が止まって DEBOUNCE_MS 経ってから 1 回だけ問い合わせる。
 * 古い問い合わせの結果は捨てる(入力が変わったあとに古い結果が出ないように)。
 */

import { useEffect, useState } from 'react';
import { fetchSimilarIncidents } from '../api/similarIncidentsApi';
import type { SimilarIncident } from '@/shared/api/types';

export const SIMILAR_DEBOUNCE_MS = 700;
/** タイトルがこの文字数に満たないうちは探さない(短すぎる入力は一致が偶然になる) */
export const SIMILAR_MIN_TITLE_CHARS = 4;

export interface UseSimilarIncidentsArgs {
  enabled: boolean;
  projectPrefix: string | null | undefined;
  title: string;
  description: string;
}

export interface SimilarIncidentsState {
  incidents: SimilarIncident[];
  /** 今の入力に対して、検索が実行できたか(0 件でも true。off・失敗・入力待ちは false) */
  searched: boolean;
}

const NOT_SEARCHED: SimilarIncidentsState = { incidents: [], searched: false };

export function useSimilarIncidents({
  enabled,
  projectPrefix,
  title,
  description,
}: UseSimilarIncidentsArgs): SimilarIncidentsState {
  // 結果には「どの入力に対するものか」を持たせ、今の入力と一致するときだけ返す。
  // (入力が変わった・無効になったときに、古い結果が出ないようにする。effect 内で状態を直接消さずに済む)
  const [result, setResult] = useState<{ key: string; incidents: SimilarIncident[] } | null>(null);

  const trimmed = title.trim();
  const active = enabled && !!projectPrefix && trimmed.length >= SIMILAR_MIN_TITLE_CHARS;
  const key = active ? `${projectPrefix}\u0000${trimmed}\u0000${description}` : null;

  useEffect(() => {
    if (key === null || !projectPrefix) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      void fetchSimilarIncidents({ projectPrefix, title: trimmed, description }).then((res) => {
        // off・失敗(null)のときは結果を持たない(案内も出さない)
        if (!cancelled) setResult(res ? { key, incidents: res.incidents } : null);
      });
    }, SIMILAR_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [key, projectPrefix, trimmed, description]);

  return key !== null && result?.key === key
    ? { incidents: result.incidents, searched: true }
    : NOT_SEARCHED;
}
