/**
 * failedChanges.test.ts — 同期失敗管理のテスト
 *
 * 前提: fake-indexeddb が import され、apiClient が vi.mock される
 */

import 'fake-indexeddb/auto';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { db, openUserDb } from './db';
import { registerPushRequester } from './pushRequester';
import { useSyncStatus } from './syncStatusStore';
import {
  listFailedChanges,
  retryFailedChange,
  discardFailedChange,
} from './failedChanges';
import { ticketDto, ticketRow, nextUserId } from './__tests__/testUtils';

// API クライアントをモック
vi.mock('../api/client', () => {
  const apiClient = {
    get: vi.fn(),
    post: vi.fn(),
    patch: vi.fn(),
    put: vi.fn(),
    delete: vi.fn(),
  };
  return { apiClient, default: apiClient };
});

describe('failedChanges', () => {
  let mockPushRequester: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    const userId = nextUserId();
    openUserDb(userId);
    await db.syncQueue.clear();
    await db.tickets.clear();
    await db.projects.clear();

    mockPushRequester = vi.fn();
    registerPushRequester(mockPushRequester);
  });

  describe('listFailedChanges', () => {
    it('should return only items with retryCount >= MAX_RETRY', async () => {
      // retryCount < MAX_RETRY: pending
      await db.syncQueue.add({
        entity: 'ticket',
        entityId: 100,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-001' }),
        createdAt: new Date().toISOString(),
        retryCount: 3,
      });

      // retryCount >= MAX_RETRY: failed
      const now = new Date();
      const failedItem = await db.syncQueue.add({
        entity: 'ticket',
        entityId: 200,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-002', patch: { title: 'Updated' } }),
        createdAt: now.toISOString(),
        retryCount: 5,
        lastError: 'Network error',
      });

      const result = await listFailedChanges();

      expect(result).toHaveLength(1);
      expect(result[0].id).toBe(failedItem);
      expect(result[0].lastError).toBe('Network error');
    });

    it('should return label with key and title for ticket update', async () => {
      // DB に チケットを追加
      await db.tickets.add(
        ticketRow(200, {}, { ticketKey: 'ABC-002', title: 'Existing Ticket' })
      );

      await db.syncQueue.add({
        entity: 'ticket',
        entityId: 200,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-002', patch: { title: 'New Title' } }),
        createdAt: new Date().toISOString(),
        retryCount: 5,
      });

      const result = await listFailedChanges();

      expect(result).toHaveLength(1);
      expect(result[0].label).toBe('ABC-002 Existing Ticket');
    });

    it('should return label with title for ticket create', async () => {
      await db.syncQueue.add({
        entity: 'ticket',
        entityId: 300,
        operation: 'create',
        payload: JSON.stringify({ body: { title: 'New Ticket' } }),
        createdAt: new Date().toISOString(),
        retryCount: 5,
      });

      const result = await listFailedChanges();

      expect(result).toHaveLength(1);
      expect(result[0].label).toBe('New Ticket');
    });

    it('should sort by createdAt ascending', async () => {
      const base = new Date('2026-09-01');

      await db.syncQueue.add({
        entity: 'ticket',
        entityId: 100,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-001' }),
        createdAt: new Date(base.getTime() + 2000).toISOString(),
        retryCount: 5,
      });

      await db.syncQueue.add({
        entity: 'ticket',
        entityId: 101,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-002' }),
        createdAt: new Date(base.getTime() + 1000).toISOString(),
        retryCount: 5,
      });

      const result = await listFailedChanges();

      expect(result).toHaveLength(2);
      expect(result[0].entityId).toBe(101); // created at 1000
      expect(result[1].entityId).toBe(100); // created at 2000
    });
  });

  describe('retryFailedChange', () => {
    it('should reset retryCount to 0 and call requestPush', async () => {
      const id = await db.syncQueue.add({
        entity: 'ticket',
        entityId: 100,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-001' }),
        createdAt: new Date().toISOString(),
        retryCount: 5,
        lastError: 'Previous error',
      });

      await retryFailedChange(id);

      const updated = await db.syncQueue.get(id);
      expect(updated?.retryCount).toBe(0);
      expect(updated?.lastError).toBeUndefined();
      expect(mockPushRequester).toHaveBeenCalled();
    });

    it('should update sync status', async () => {
      const id = await db.syncQueue.add({
        entity: 'ticket',
        entityId: 100,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-001' }),
        createdAt: new Date().toISOString(),
        retryCount: 5,
      });

      await useSyncStatus.getState().refreshQueueCounts();
      const stateBefore = useSyncStatus.getState();
      expect(stateBefore.failedCount).toBe(1);

      await retryFailedChange(id);

      const stateAfter = useSyncStatus.getState();
      expect(stateAfter.failedCount).toBe(0);
    });
  });

  describe('discardFailedChange', () => {
    it('should delete queue item for create', async () => {
      const id = await db.syncQueue.add({
        entity: 'ticket',
        entityId: 300,
        operation: 'create',
        payload: JSON.stringify({ body: { title: 'Temp Ticket' } }),
        createdAt: new Date().toISOString(),
        retryCount: 5,
      });

      // 仮行を追加
      await db.tickets.add(ticketRow(300, {}, { _pendingCreate: true }));

      await discardFailedChange(id);

      const queueItem = await db.syncQueue.get(id);
      expect(queueItem).toBeUndefined();

      const ticket = await db.tickets.get(300);
      expect(ticket).toBeUndefined();
    });

    it('should restore ticket from server for update', async () => {
      const { apiClient } = await import('../api/client');
      vi.mocked(apiClient.get).mockResolvedValueOnce({
        data: ticketDto(200, { title: 'Server Version' }),
      });

      const id = await db.syncQueue.add({
        entity: 'ticket',
        entityId: 200,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-002', patch: { title: 'Local Change' } }),
        createdAt: new Date().toISOString(),
        retryCount: 5,
      });

      // 端末側の変更を入れる
      await db.tickets.add(
        ticketRow(200, { title: 'Local Change' }, { _dirty: true })
      );

      await discardFailedChange(id);

      const queueItem = await db.syncQueue.get(id);
      expect(queueItem).toBeUndefined();

      const ticket = await db.tickets.get(200);
      expect(ticket?.title).toBe('Server Version');
      expect(ticket?._dirty).toBe(false);
      expect(ticket?._syncError).toBeNull();
    });

    it('should delete ticket if 404 on update', async () => {
      const { apiClient } = await import('../api/client');
      vi.mocked(apiClient.get).mockRejectedValueOnce({
        response: { status: 404 },
      });

      const id = await db.syncQueue.add({
        entity: 'ticket',
        entityId: 200,
        operation: 'update',
        payload: JSON.stringify({ key: 'ABC-002' }),
        createdAt: new Date().toISOString(),
        retryCount: 5,
      });

      await db.tickets.add(ticketRow(200));

      await discardFailedChange(id);

      const ticket = await db.tickets.get(200);
      expect(ticket).toBeUndefined();

      const queueItem = await db.syncQueue.get(id);
      expect(queueItem).toBeUndefined();
    });

    it('should update sync status after discard', async () => {
      const id = await db.syncQueue.add({
        entity: 'ticket',
        entityId: 100,
        operation: 'create',
        payload: JSON.stringify({ body: { title: 'Test' } }),
        createdAt: new Date().toISOString(),
        retryCount: 5,
      });

      await db.tickets.add(ticketRow(100, {}, { _pendingCreate: true }));

      await useSyncStatus.getState().refreshQueueCounts();
      const stateBefore = useSyncStatus.getState();
      expect(stateBefore.failedCount).toBe(1);

      await discardFailedChange(id);

      const stateAfter = useSyncStatus.getState();
      expect(stateAfter.failedCount).toBe(0);
    });
  });
});
