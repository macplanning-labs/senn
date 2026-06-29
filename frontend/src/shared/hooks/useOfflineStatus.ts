/**
 * useOfflineStatus.ts — オフライン状態フック
 *
 * navigator.onLine + オンライン/オフラインイベントを監視。
 * UIに接続状態を表示するために使用。
 */

import { useState, useEffect } from 'react';

export function useOfflineStatus() {
  const [isOnline, setIsOnline] = useState(navigator.onLine);

  useEffect(() => {
    const handleOnline = () => setIsOnline(true);
    const handleOffline = () => setIsOnline(false);

    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);

    return () => {
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
    };
  }, []);

  return { isOnline, isOffline: !isOnline };
}
