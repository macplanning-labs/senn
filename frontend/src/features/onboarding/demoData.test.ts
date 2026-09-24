/**
 * demoData.test.ts — サンプルデータ生成のテスト
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { formatDateLocal, isDemoProject, deleteDemoProject, createDemoData } from './demoData';
import i18n from '@/i18n';
import { apiClient } from '@/shared/api/client';

// apiClient をモック
vi.mock('@/shared/api/client', () => ({
  apiClient: {
    post: vi.fn(),
    delete: vi.fn(),
  },
}));

describe('demoData', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('formatDateLocal', () => {
    it('日付を YYYY-MM-DD にローカル時間で整形する', () => {
      const date = new Date('2026-09-19T12:00:00');
      const result = formatDateLocal(date);
      expect(result).toBe('2026-09-19');
    });

    it('1 桁の月日をパディングする', () => {
      const date = new Date('2026-01-05T12:00:00');
      const result = formatDateLocal(date);
      expect(result).toBe('2026-01-05');
    });
  });

  describe('isDemoProject', () => {
    it('DEMO prefix かつ名前が「サンプルプロジェクト」なら真', () => {
      const result = isDemoProject({
        prefix: 'DEMO',
        name: 'サンプルプロジェクト',
      });
      expect(result).toBe(true);
    });

    it('DEMO prefix かつ名前が「Sample project」なら真', () => {
      const result = isDemoProject({
        prefix: 'DEMO',
        name: 'Sample project',
      });
      expect(result).toBe(true);
    });

    it('DEMO2 prefix かつ名前が一致なら真', () => {
      const result = isDemoProject({
        prefix: 'DEMO2',
        name: 'サンプルプロジェクト',
      });
      expect(result).toBe(true);
    });

    it('prefix が DEMO でも名前が異なれば偽', () => {
      const result = isDemoProject({
        prefix: 'DEMO',
        name: 'My Project',
      });
      expect(result).toBe(false);
    });

    it('prefix が DEMO でなければ偽', () => {
      const result = isDemoProject({
        prefix: 'MYAPP',
        name: 'サンプルプロジェクト',
      });
      expect(result).toBe(false);
    });

    it('大文字小文字は無視', () => {
      const result = isDemoProject({
        prefix: 'demo',
        name: 'サンプルプロジェクト',
      });
      expect(result).toBe(true);
    });
  });

  describe('deleteDemoProject', () => {
    it('bulk-delete → delete の順に呼ぶ', async () => {
      const mockPost = vi.mocked(apiClient.post);
      const _mockDelete = vi.mocked(apiClient.delete);

      await deleteDemoProject(123);

      expect(mockPost).toHaveBeenCalledWith('/tickets/bulk-delete/', {
        project_id: 123,
        delete_all: true,
      });
      expect(_mockDelete).toHaveBeenCalledWith('/projects/123/');
    });

    it('bulk-delete が失敗したら delete を呼ばない', async () => {
      const mockPost = vi.mocked(apiClient.post);
      const _mockDelete = vi.mocked(apiClient.delete);
      mockPost.mockRejectedValueOnce(new Error('bulk-delete failed'));

      await expect(deleteDemoProject(123)).rejects.toThrow('bulk-delete failed');
      expect(_mockDelete).not.toHaveBeenCalled();
    });

    it('delete が失敗したら例外を投げる', async () => {
      const mockDelete = vi.mocked(apiClient.delete);
      mockDelete.mockRejectedValueOnce(new Error('delete failed'));

      await expect(deleteDemoProject(123)).rejects.toThrow('delete failed');
    });
  });

  describe('createDemoData', () => {
    it('DEMO prefix が使用可能なら使う', async () => {
      const mockPost = vi.mocked(apiClient.post);
      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValue({ data: { id: 200, ticketKey: 'DEMO-1' } }); // tickets

      const result = await createDemoData(10, ['OTHER']);
      expect(result.prefix).toBe('DEMO');
      expect(result.projectId).toBe(100);
    });

    it('Cycle 作成に project と teamId を渡す', async () => {
      const mockPost = vi.mocked(apiClient.post);
      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValue({ data: { id: 200, ticketKey: 'DEMO-1' } }); // tickets

      await createDemoData(10, []);

      const cycleCall = mockPost.mock.calls.find((c) => c[0] === '/cycles/');
      expect(cycleCall?.[1]).toEqual(
        expect.objectContaining({ project: 100, teamId: 10 }),
      );
    });

    it('チケットの status は新規チームの既定ステータスだけを使う', async () => {
      const mockPost = vi.mocked(apiClient.post);
      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValue({ data: { id: 200, ticketKey: 'DEMO-1' } }); // tickets

      await createDemoData(10, []);

      const defaultStatuses = ['backlog', 'open', 'in_progress', 'resolved', 'closed', 'canceled'];
      const statuses = mockPost.mock.calls
        .filter((c) => c[0] === '/tickets/')
        .map((c) => (c[1] as { status: string }).status);
      expect(statuses).toHaveLength(8);
      for (const status of statuses) {
        expect(defaultStatuses).toContain(status);
      }
    });

    it('DEMO が使用済みなら DEMO2 を使う', async () => {
      const mockPost = vi.mocked(apiClient.post);
      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValue({ data: { id: 200, ticketKey: 'DEMO2-1' } }); // tickets

      const result = await createDemoData(10, ['DEMO', 'OTHER']);
      expect(result.prefix).toBe('DEMO2');
    });

    it('大文字小文字を無視して prefix 決定', async () => {
      const mockPost = vi.mocked(apiClient.post);
      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValue({ data: { id: 200, ticketKey: 'DEMO2-1' } }); // tickets

      const result = await createDemoData(10, ['demo']);
      expect(result.prefix).toBe('DEMO2');
    });

    it('DEMO〜DEMO20 が全て使用済みならエラー', async () => {
      const used = Array.from({ length: 21 }, (_, i) =>
        i === 0 ? 'DEMO' : `DEMO${i}`,
      );
      await expect(createDemoData(10, used)).rejects.toThrow(
        'All DEMO prefixes (DEMO to DEMO20) are in use',
      );
    });

    it('言語が ja で始まるなら JA タイトルを使う', async () => {
      const mockPost = vi.mocked(apiClient.post);
      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValue({ data: { id: 200, ticketKey: 'DEMO-1' } }); // tickets

      // i18n.language = 'ja-JP' に変更
      const originalLang = i18n.language;
      i18n.changeLanguage('ja-JP');

      try {
        await createDemoData(10, []);

        // チケット作成時のペイロルドで JA タイトルが使われているか確認
        const calls = vi.mocked(apiClient.post).mock.calls;
        const ticketCalls = calls.filter((call) => call[0] === '/tickets/');
        const ticketPayload = ticketCalls[0]?.[1];
        expect(ticketPayload).toEqual(
          expect.objectContaining({
            title: '要件を整理する',
          }),
        );
      } finally {
        i18n.changeLanguage(originalLang);
      }
    });

    it('言語が en なら EN タイトルを使う', async () => {
      const mockPost = vi.mocked(apiClient.post);
      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValue({ data: { id: 200, ticketKey: 'DEMO-1' } }); // tickets

      const originalLang = i18n.language;
      i18n.changeLanguage('en');

      try {
        await createDemoData(10, []);

        const calls = vi.mocked(apiClient.post).mock.calls;
        const ticketCalls = calls.filter((call) => call[0] === '/tickets/');
        const ticketPayload = ticketCalls[0]?.[1];
        expect(ticketPayload).toEqual(
          expect.objectContaining({
            title: 'Define requirements',
          }),
        );
      } finally {
        i18n.changeLanguage(originalLang);
      }
    });

    it('チケット作成が 8 件あり、依存関係が 7 本', async () => {
      const mockPost = vi.mocked(apiClient.post);
      const mockDelete = vi.mocked(apiClient.delete);

      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValueOnce({ data: { id: 201, ticketKey: 'DEMO-1' } })
        .mockResolvedValueOnce({ data: { id: 202, ticketKey: 'DEMO-2' } })
        .mockResolvedValueOnce({ data: { id: 203, ticketKey: 'DEMO-3' } })
        .mockResolvedValueOnce({ data: { id: 204, ticketKey: 'DEMO-4' } })
        .mockResolvedValueOnce({ data: { id: 205, ticketKey: 'DEMO-5' } })
        .mockResolvedValueOnce({ data: { id: 206, ticketKey: 'DEMO-6' } })
        .mockResolvedValueOnce({ data: { id: 207, ticketKey: 'DEMO-7' } })
        .mockResolvedValueOnce({ data: { id: 208, ticketKey: 'DEMO-8' } })
        .mockResolvedValue({}); // dependencies

      await createDemoData(10, []);

      const calls = vi.mocked(apiClient.post).mock.calls;
      const ticketCalls = calls.filter((call) => call[0] === '/tickets/');
      const depCalls = calls.filter((call) =>
        (call[0] as string).includes('/dependencies/'),
      );

      expect(ticketCalls).toHaveLength(8);
      expect(depCalls).toHaveLength(7);
      expect(mockDelete).not.toHaveBeenCalled();
    });

    it('チケット作成途中で失敗したら deleteDemoProject を呼んで巻き戻す', async () => {
      const mockPost = vi.mocked(apiClient.post);
      const mockDelete = vi.mocked(apiClient.delete);

      mockPost
        .mockResolvedValueOnce({ data: { id: 100 } }) // project
        .mockResolvedValueOnce({ data: { id: 1 } }) // cycle
        .mockResolvedValueOnce({ data: { id: 201, ticketKey: 'DEMO-1' } })
        .mockResolvedValueOnce({ data: { id: 202, ticketKey: 'DEMO-2' } })
        .mockRejectedValueOnce(new Error('ticket 3 creation failed')); // ticket 3 が失敗

      await expect(createDemoData(10, [])).rejects.toThrow(
        'ticket 3 creation failed',
      );

      // bulk-delete と delete が呼ばれているか確認
      expect(mockPost).toHaveBeenCalledWith('/tickets/bulk-delete/', {
        project_id: 100,
        delete_all: true,
      });
      expect(mockDelete).toHaveBeenCalledWith('/projects/100/');
    });
  });
});
