/**
 * RoadmapDetail.test.ts — ロードマップ詳細画面のテスト
 *
 * renderToStaticMarkup で、canManage の true/false で編集・削除ボタンの有無を検証
 */

import { describe, it, expect, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import type { RoadmapDetail } from '@/shared/api/types';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useParams: () => ({ id: '1' }),
    useNavigate: () => vi.fn(),
  };
});

let mockRoadmapDetail: RoadmapDetail | null = null;

vi.mock('../hooks/useProjectStructure', () => ({
  useRoadmapDetail: () => ({
    data: mockRoadmapDetail,
    isLoading: false,
    isError: false,
  }),
  useUpdateRoadmap: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  useDeleteRoadmap: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  projectStructureKey: vi.fn(),
  roadmapsKey: vi.fn(),
  roadmapKey: vi.fn(),
}));

vi.mock('../utils/projectProgress', () => ({
  formatProgress: (progress: number | null) => progress === null ? '—' : `${Math.round(progress * 100)}%`,
}));

vi.mock('./RoadmapDetail.css', () => ({}));

import { RoadmapDetail as RoadmapDetailComponent } from './RoadmapDetail';

function renderComponent(roadmap: RoadmapDetail | null): string {
  mockRoadmapDetail = roadmap;
  return renderToStaticMarkup(
    createElement(MemoryRouter, null, createElement(RoadmapDetailComponent)),
  );
}

describe('RoadmapDetail', () => {
  const baseRoadmap: RoadmapDetail = {
    id: 1,
    name: '2026年下期',
    description: '下期の重点施策',
    ownerId: 10,
    canManage: false,
    projects: [
      {
        id: 101,
        prefix: 'P1',
        name: 'Project 1',
        status: 'active',
        ticketCount: 20,
        completedCount: 10,
        progress: 0.5,
      },
    ],
  };

  it('canManage=false の場合、編集・削除ボタンが出ない', () => {
    const roadmap = { ...baseRoadmap, canManage: false };
    const html = renderComponent(roadmap);
    expect(html).not.toContain('data-testid="roadmap-edit-btn"');
    expect(html).not.toContain('data-testid="roadmap-delete-btn"');
  });

  it('canManage=true の場合、編集・削除ボタンが出現', () => {
    const roadmap = { ...baseRoadmap, canManage: true };
    const html = renderComponent(roadmap);
    expect(html).toContain('data-testid="roadmap-edit-btn"');
    expect(html).toContain('data-testid="roadmap-delete-btn"');
  });

  it('プロジェクトが空の場合、空表示が出現', () => {
    const roadmap = { ...baseRoadmap, projects: [] };
    const html = renderComponent(roadmap);
    expect(html).toContain('roadmaps.projectsEmpty');
    expect(html).toContain('data-testid="roadmap-projects-empty"');
  });

  it('プロジェクトの進捗が正しく表示される', () => {
    const roadmap: RoadmapDetail = {
      ...baseRoadmap,
      projects: [
        {
          id: 101,
          prefix: 'P1',
          name: 'Project 1',
          status: 'completed',
          ticketCount: 10,
          completedCount: 10,
          progress: 1,
        },
        {
          id: 102,
          prefix: 'P2',
          name: 'Project 2',
          status: 'paused',
          ticketCount: 5,
          completedCount: 0,
          progress: null,
        },
      ],
    };
    const html = renderComponent(roadmap);
    expect(html).toContain('100%');
    expect(html).toContain('—');
  });

  it('所属プロジェクトのステータスは、実在する翻訳キーで表示される(in_progress を含む)', () => {
    const roadmap: RoadmapDetail = {
      ...baseRoadmap,
      projects: [
        { id: 103, prefix: 'P3', name: 'Project 3', status: 'in_progress', ticketCount: 1, completedCount: 0, progress: 0 },
      ],
    };
    const html = renderComponent(roadmap);
    expect(html).toContain('sidebar.projectStatus.inProgress');
    expect(html).not.toContain('sidebar.projectStatus.in_progress');
  });

  it('削除ボタンが出現する（canManage=true のとき）', () => {
    const roadmap = { ...baseRoadmap, canManage: true };
    const html = renderComponent(roadmap);
    expect(html).toContain('data-testid="roadmap-delete-btn"');
    // 削除確認はユーザー操作で出現するため、初期状態では出現しない
    expect(html).not.toContain('data-testid="roadmap-confirm-delete"');
  });

  it('ロードマップが見つからない場合、エラーメッセージが出現', () => {
    const html = renderComponent(null);
    expect(html).toContain('roadmaps.notFound');
  });
});

describe('RoadmapDetail: 画面からの行き来(UI導線)', () => {
  it('ロードマップ一覧へ戻るリンクがある', () => {
    const html = renderComponent({
      id: 1, name: 'R', description: '', ownerId: 1, canManage: false, projects: [],
    } as unknown as RoadmapDetail);
    expect(html).toMatch(/href="\/roadmaps"[^>]*data-testid="roadmap-back-to-list"|data-testid="roadmap-back-to-list"[^>]*href="\/roadmaps"/);
  });
});
