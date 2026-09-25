/**
 * useLiveRows.ts — 端末内 DB（Dexie）の問い合わせ結果を購読する React フック
 *
 * - DB が切り替わったら（ユーザー変更）購読し直す
 * - 条件が変わっても、次の結果が出るまで前の結果を出し続ける（入力のたびに一覧が空になってちらつかない）
 */
import { liveQuery } from 'dexie';
import { useEffect, useState } from 'react';
import { useDbGeneration } from '../db';

export interface LiveResult<T> {
  data: T;
  /** 一度でも結果が出たか */
  loaded: boolean;
}

export function useLiveRows<T>(querier: () => Promise<T>, deps: readonly unknown[], initial: T): LiveResult<T> {
  const generation = useDbGeneration();
  const [state, setState] = useState<LiveResult<T>>({ data: initial, loaded: false });

  useEffect(() => {
    let active = true;
    const sub = liveQuery(querier).subscribe({
      next: (value) => {
        if (active) setState({ data: value, loaded: true });
      },
      error: (err) => {
        console.warn('[local-first] liveQuery failed', err);
        if (active) setState((s) => ({ ...s, loaded: true }));
      },
    });
    return () => {
      active = false;
      sub.unsubscribe();
    };
    // querier は deps で表される前提（呼び出し側が deps を正しく渡す）
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [generation, ...deps]);

  return state;
}
