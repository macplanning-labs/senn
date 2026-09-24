import { useEffect, useState } from 'react';
import type { Observable } from 'dexie';

/**
 * Dexie liveQuery の結果を React ステートで管理
 *
 * Observable（liveQueryの戻り値）をサブスクライブして、
 * 変更があるたびに setState を実行
 */
export function useObservable<T>(observable: Observable<T> | null, initialValue: T): T {
  const [data, setData] = useState<T>(initialValue);

  useEffect(() => {
    if (!observable) {
      setData(initialValue);
      return;
    }

    setData(initialValue);

    const subscription = observable.subscribe((result) => {
      setData(result);
    });

    return () => {
      subscription.unsubscribe();
    };
  }, [observable]);

  return data;
}
