/**
 * syncStatusStore.ts — 同期状態ストア（zustand）
 *
 * ローカルファースト同期エンジンの状態管理。
 * UI から同期の進捗を監視するために使用。
 */

import { create } from 'zustand';
import { db } from './db';

export interface SyncStatusState {
  /** フル同期が1回以上完了した */
  initialSyncDone: boolean;
  /** 現在同期中 */
  syncing: boolean;
  /** 最後の同期日時 */
  lastSyncAt: string | null;
  /** 最後のエラーメッセージ */
  lastError: string | null;
  /** 未送信（retryCount<5）の項目数 */
  pendingCount: number;
  /** 送信失敗（retryCount>=5）の項目数 */
  failedCount: number;
  /** 部分更新 */
  set(partial: Partial<SyncStatusState>): void;
  /** キュー件数をリフレッシュ */
  refreshQueueCounts(): Promise<void>;
}

export const useSyncStatus = create<SyncStatusState>((set) => ({
  initialSyncDone: false,
  syncing: false,
  lastSyncAt: null,
  lastError: null,
  pendingCount: 0,
  failedCount: 0,

  set: (partial) => {
    set(partial);
  },

  refreshQueueCounts: async () => {
    try {
      const queue = await db.syncQueue.toArray();
      let pending = 0;
      let failed = 0;

      for (const item of queue) {
        const retryCount = item.retryCount ?? 0;
        if (retryCount >= 5) {
          failed++;
        } else {
          pending++;
        }
      }

      set({ pendingCount: pending, failedCount: failed });
    } catch {
      // DB 読み取り失敗時は無視
    }
  },
}));
