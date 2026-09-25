/**
 * demoMode.test.ts — デモモード単体テスト
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';

const localStorageStore: Record<string, string> = {};

const localStorageMock = {
  getItem: (key: string) => localStorageStore[key] ?? null,
  setItem: (key: string, value: string) => {
    localStorageStore[key] = value;
  },
  removeItem: (key: string) => {
    delete localStorageStore[key];
  },
  clear: () => {
    Object.keys(localStorageStore).forEach((key) => delete localStorageStore[key]);
  },
};

Object.defineProperty(globalThis, 'localStorage', {
  value: localStorageMock,
  writable: true,
});

import {
  isDemoMode,
  enableDemoMode,
  disableDemoMode,
  clearDemoMode,
} from './demoMode';
import { handleDemoRequest } from './demoApiAdapter';
import { demoUser, demoProject, demoTeam } from './demoFixtures';

describe('demoMode', () => {
  beforeEach(() => {
    localStorageMock.clear();
  });

  afterEach(() => {
    clearDemoMode();
  });

  describe('flag management', () => {
    it('should return false when demo mode is not enabled', () => {
      expect(isDemoMode()).toBe(false);
    });

    it('should return true after enableDemoMode', () => {
      enableDemoMode();
      expect(isDemoMode()).toBe(true);
    });

    it('should return false after disableDemoMode', () => {
      enableDemoMode();
      disableDemoMode();
      expect(isDemoMode()).toBe(false);
    });

    it('should return false when TTL expires', () => {
      enableDemoMode();
      expect(isDemoMode()).toBe(true);

      // TTL を過去に設定（30分以上前）
      const pastTime = Date.now() - (31 * 60 * 1000);
      localStorageMock.setItem('senn_demo_started_at', pastTime.toString());

      // isDemoMode() は false を返し、clearDemoMode を呼ぶ
      expect(isDemoMode()).toBe(false);

      // localStorage も cleared している
      expect(localStorageMock.getItem('senn_demo_mode')).toBeNull();
      expect(localStorageMock.getItem('senn_demo_started_at')).toBeNull();
    });

    it('should keep demo mode within TTL', () => {
      enableDemoMode();
      expect(isDemoMode()).toBe(true);

      // TTL 内の時刻を設定（5分前）
      const recentTime = Date.now() - (5 * 60 * 1000);
      localStorageMock.setItem('senn_demo_started_at', recentTime.toString());

      // isDemoMode() は true
      expect(isDemoMode()).toBe(true);
    });
  });

  describe('demoApiAdapter', () => {
    it('should return null when demo mode is not enabled', () => {
      const result = handleDemoRequest({
        method: 'get',
        url: '/auth/me/',
      });
      expect(result).toBeNull();
    });

    describe('when demo mode is enabled', () => {
      beforeEach(() => {
        enableDemoMode();
      });

      it('should return demo user for /auth/me/', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/auth/me/',
        });
        expect(result).not.toBeNull();
        expect(result?.status).toBe(200);
        expect(result?.data).toEqual(demoUser);
      });

      it('should return raw my-tickets array with ticket_key fields', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/dashboard/my-tickets/',
        });
        expect(result).not.toBeNull();
        expect(result?.status).toBe(200);
        expect(Array.isArray(result?.data)).toBe(true);
        const tickets = result?.data as Array<{ ticket_key: string }>;
        expect(tickets.length).toBeGreaterThanOrEqual(4);
        expect(tickets[0]).toHaveProperty('ticket_key');
      });

      it('should return demo project for /projects/', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/projects/',
        });
        expect(result?.status).toBe(200);
        expect((result?.data as { results: unknown[] }).results).toHaveLength(1);
        expect((result?.data as { results: typeof demoProject[] }).results[0]).toEqual(demoProject);
      });

      it('should return demo team for /teams/', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/teams/',
        });
        expect(result?.status).toBe(200);
        expect((result?.data as { results: unknown[] }).results).toHaveLength(1);
        expect((result?.data as { results: typeof demoTeam[] }).results[0]).toEqual(demoTeam);
      });

      it('should return ticket detail by ticket_key', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/tickets/DEMO-1/',
        });
        expect(result?.status).toBe(200);
        expect(result?.data).toHaveProperty('ticketKey', 'DEMO-1');
        expect(result?.data).toHaveProperty('author');
        expect(result?.data).not.toHaveProperty('reporter');
      });

      it('should return ticket detail by numeric id', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/tickets/101/',
        });
        expect(result?.status).toBe(200);
        expect(result?.data).toHaveProperty('ticketKey', 'DEMO-1');
      });

      it('should return status 200 for unknown GET paths', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/some/unknown/endpoint/',
        });
        expect(result).not.toBeNull();
        expect(result?.status).toBe(200);
      });

      it('should return empty change-logs', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/tickets/DEMO-1/change-logs/',
        });
        expect(result?.status).toBe(200);
        expect(result?.data).toEqual([]);
      });

      it('should reject POST with 403', () => {
        const result = handleDemoRequest({
          method: 'post',
          url: '/tickets/',
          data: { title: 'New ticket' },
        });
        expect(result).not.toBeNull();
        expect(result?.status).toBe(403);
      });

      it('should return sync DTO array for /sync/tickets/', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/sync/tickets/?cursor=demo&limit=500',
        });
        expect(result?.status).toBe(200);
        const data = result?.data as any;
        expect(data.changes).toBeInstanceOf(Array);
        expect(data.changes.length).toBeGreaterThan(0);
        expect(data.deleted).toEqual([]);
        expect(data.access).toEqual({ all: true, teamIds: [], scopedProjects: [] });
        expect(data.cursor).toBe('demo');
        expect(data.hasMore).toBe(false);
        expect(data.serverTime).toBeDefined();
        // 最初の change が DTO 形式
        expect(data.changes[0]).toHaveProperty('id');
        expect(data.changes[0]).toHaveProperty('ticketKey');
        expect(data.changes[0]).toHaveProperty('title');
      });

      it('should return sync DTO for /sync/projects/', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/sync/projects/?cursor=demo&limit=500',
        });
        expect(result?.status).toBe(200);
        const data = result?.data as any;
        expect(data.changes).toBeInstanceOf(Array);
        expect(data.changes.length).toBe(1);
        expect(data.changes[0]).toHaveProperty('id');
        expect(data.changes[0]).toHaveProperty('name');
        expect(data.deleted).toEqual([]);
        expect(data.access).toEqual({ all: true, teamIds: [], scopedProjects: [] });
        expect(data.cursor).toBe('demo');
        expect(data.hasMore).toBe(false);
      });

      it('should return extras for /tickets/{key}/extras/', () => {
        const result = handleDemoRequest({
          method: 'get',
          url: '/tickets/DEMO-1/extras/',
        });
        expect(result?.status).toBe(200);
        const data = result?.data as any;
        expect(data).toHaveProperty('comments');
        expect(data).toHaveProperty('attachments');
        expect(data).toHaveProperty('links');
        expect(data).toHaveProperty('linkedWikiPages');
        expect(data).toHaveProperty('isWatching');
        expect(data.isWatching).toBe(false);
      });
    });
  });
});
