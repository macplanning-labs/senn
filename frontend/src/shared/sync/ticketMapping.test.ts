import { describe, it, expect } from 'vitest';
import { toLocalTicket, toLocalProject } from './ticketMapping';

describe('ticketMapping', () => {
  describe('toLocalTicket', () => {
    it('詳細 DTO（comments等を含む）を入れて付随データが消え、索引用項目が作られる', () => {
      const detailDto = {
        id: 123,
        ticketKey: 'ABC-001',
        title: 'Test Ticket',
        description: 'Test Description',
        status: 'open',
        priority: 'high',
        ticketType: 'task',
        assignees: [{ id: 1, username: 'user1', displayName: 'User 1' }],
        reviewers: [{ id: 2, username: 'user2', displayName: 'User 2' }],
        author: { id: 3, username: 'user3', displayName: 'User 3' },
        category: { id: 10, name: 'Category 1', slug: 'cat1', color: '#fff' },
        milestone: { id: 20, name: 'v1.0', dueDate: '2026-12-31' },
        project: 5,
        projectPrefix: 'ABC',
        projectName: 'Project ABC',
        parent: null,
        labels: [{ id: 30, name: 'bug', color: '#ff0000' }],
        startDate: '2026-01-01',
        dueDate: '2026-12-31',
        storyPoints: 5,
        cycle: 100,
        cycleName: 'Cycle 1',
        team: { id: 7, name: 'Team A', slug: 'team-a', icon: 'icon', color: '#000' },
        commentCount: 3,
        childCount: 2,
        totalTimeSpent: 480,
        gantt_order: 10,
        createdAt: '2026-01-01T10:00:00Z',
        updatedAt: '2026-09-25T10:00:00Z',
        closedAt: null,
        // 付随項目（削除されるべき）
        comments: [{ id: 1, body: 'comment', author: { id: 1 } }],
        attachments: [{ id: 1, url: 'http://example.com' }],
        links: [],
        linkedRules: [],
        linkedWikiPages: [],
        isWatching: true,
      };

      const result = toLocalTicket(detailDto);

      // 付随データが消えている
      expect((result as any).comments).toBeUndefined();
      expect((result as any).attachments).toBeUndefined();
      expect((result as any).links).toBeUndefined();
      expect((result as any).isWatching).toBeUndefined();

      // 基本項目が正しい
      expect(result.id).toBe(123);
      expect(result.ticketKey).toBe('ABC-001');
      expect(result.title).toBe('Test Ticket');
      expect(result.status).toBe('open');

      // 索引用項目が作られている
      expect(result.teamId).toBe(7);
      expect(result.projectId).toBe(5);
      expect(result.cycleId).toBe(100);
      expect(result.parentId).toBeNull();
      expect(result.assigneeIds).toEqual([1]);
      expect(result.labelIds).toEqual([30]);

      // ローカルメタデータが付いている
      expect(result._dirty).toBe(false);
      expect(result._syncedAt).toBeDefined();
    });

    it('不足した項目に安全な既定値を使う', () => {
      const minimalDto = { id: 1, ticketKey: 'ABC-001' };
      const result = toLocalTicket(minimalDto);

      expect(result.title).toBe('');
      expect(result.description).toBe('');
      expect(result.status).toBe('open');
      expect(result.assignees).toEqual([]);
      expect(result.labels).toEqual([]);
      expect(result.dueDate).toBeNull();
      expect(result.commentCount).toBe(0);
      expect(result.assigneeIds).toEqual([]);
      expect(result.labelIds).toEqual([]);
    });

    it('gantt_order は dto.gantt_order ?? dto.ganttOrder ?? 0', () => {
      const dto1 = { id: 1, ticketKey: 'A-1', gantt_order: 5 };
      const dto2 = { id: 2, ticketKey: 'A-2', ganttOrder: 10 };
      const dto3 = { id: 3, ticketKey: 'A-3' };

      expect(toLocalTicket(dto1).gantt_order).toBe(5);
      expect(toLocalTicket(dto2).gantt_order).toBe(10);
      expect(toLocalTicket(dto3).gantt_order).toBe(0);
    });

    it('メタパラメータで上書きできる', () => {
      const dto = { id: 1, ticketKey: 'A-1' };
      const result = toLocalTicket(dto, { _dirty: true, _syncError: 'Failed' });

      expect(result._dirty).toBe(true);
      expect(result._syncError).toBe('Failed');
    });
  });

  describe('toLocalProject', () => {
    it('ProjectSyncDto を LocalProject に変換', () => {
      const dto = {
        id: 5,
        name: 'Project ABC',
        projectKey: 'ABC',
        description: 'Test project',
        status: 'active',
        targetStartDate: '2026-01-01',
        targetEndDate: '2026-12-31',
        isActive: true,
        gracePeriodDays: 7,
        teams: [{ id: 1, name: 'Team A', slug: 'team-a', icon: 'icon', color: '#000' }],
        cycleAutoComplete: false,
        cycleAutoCreateNext: true,
        createdAt: '2026-01-01T10:00:00Z',
        updatedAt: '2026-09-25T10:00:00Z',
      };

      const result = toLocalProject(dto);

      expect(result.id).toBe(5);
      expect(result.name).toBe('Project ABC');
      expect(result.status).toBe('active');
      expect(result._dirty).toBe(false);
      expect(result._syncedAt).toBeDefined();
    });
  });
});
