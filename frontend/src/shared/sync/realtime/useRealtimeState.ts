/**
 * useRealtimeState.ts — リアルタイム接続状態 React Hook
 */

import { useSyncExternalStore } from 'react';
import { subscribeRealtimeState, getEffectiveState } from './realtimeStatus';

export function useRealtimeState() {
  return useSyncExternalStore(subscribeRealtimeState, getEffectiveState, getEffectiveState);
}
