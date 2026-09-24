import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

// テスト環境は node のため、DOM ではなくサーバーレンダリングした HTML を検証する
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock('./ProjectUpdates.css', () => ({}));

let mockUser: { id: number; isStaff: boolean } | null = null;
vi.mock('@/shared/stores/authStore', () => ({
  useAuthStore: (sel: (s: { user: typeof mockUser }) => unknown) => sel({ user: mockUser }),
}));

let mockState: { data?: { results: unknown[] }; isLoading: boolean; isError: boolean } = {
  data: { results: [] },
  isLoading: false,
  isError: false,
};
const mutation = { mutate: vi.fn(), isPending: false };
vi.mock('../hooks/useProjectActivity', () => ({
  HEALTH_VALUES: ['on_track', 'at_risk', 'off_track'],
  useProjectUpdates: () => mockState,
  usePostProjectUpdate: () => mutation,
  useEditProjectUpdate: () => mutation,
  useDeleteProjectUpdate: () => mutation,
}));

import { ProjectUpdatesSection } from './ProjectUpdatesSection';

const update = (over: Record<string, unknown> = {}) => ({
  id: 1,
  projectId: 10,
  health: 'at_risk',
  body: '遅れ気味です',
  authorId: 5,
  authorName: '田中',
  createdAt: '2026-09-19T01:00:00Z',
  updatedAt: '2026-09-19T01:00:00Z',
  ...over,
});

const render = (isMember: boolean) =>
  renderToStaticMarkup(createElement(ProjectUpdatesSection, { projectId: 10, isMember }));

describe('ProjectUpdatesSection', () => {
  beforeEach(() => {
    mockUser = { id: 5, isStaff: false };
    mockState = { data: { results: [] }, isLoading: false, isError: false };
  });

  it('進捗が無いときは空状態を表示する', () => {
    expect(render(true)).toContain('project-updates-empty');
  });

  it('メンバーには「進捗を投稿」ボタンが出て、メンバーでない一般ユーザーには出ない', () => {
    expect(render(true)).toContain('project-update-open');
    expect(render(false)).not.toContain('project-update-open');
  });

  it('投稿できないユーザーには、理由の案内文を表示する(メンバーには出さない)', () => {
    expect(render(false)).toContain('project-updates-member-hint');
    expect(render(true)).not.toContain('project-updates-member-hint');
    mockUser = { id: 9, isStaff: true };
    expect(render(false)).not.toContain('project-updates-member-hint');
  });

  it('メンバーでなくても管理者には投稿ボタンが出る', () => {
    mockUser = { id: 9, isStaff: true };
    expect(render(false)).toContain('project-update-open');
  });

  it('進捗の健全性ラベル・本文・投稿者を表示する', () => {
    mockState = { data: { results: [update()] }, isLoading: false, isError: false };
    const html = render(true);
    expect(html).toContain('project-updates__badge--at_risk');
    expect(html).toContain('projectUpdates.health.at_risk');
    expect(html).toContain('遅れ気味です');
    expect(html).toContain('田中');
  });

  it('編集・削除は、投稿者と管理者にだけ表示する', () => {
    mockState = { data: { results: [update({ authorId: 5 })] }, isLoading: false, isError: false };
    expect(render(true)).toContain('projectUpdates.edit');

    mockUser = { id: 6, isStaff: false }; // 別の一般ユーザー
    expect(render(true)).not.toContain('projectUpdates.edit');

    mockUser = { id: 7, isStaff: true }; // 管理者
    expect(render(true)).toContain('projectUpdates.edit');
  });

  it('編集済みの進捗には「編集済み」を表示する', () => {
    mockState = {
      data: { results: [update({ updatedAt: '2026-09-19T02:00:00Z' })] },
      isLoading: false,
      isError: false,
    };
    expect(render(true)).toContain('projectUpdates.edited');
  });

  it('読み込み失敗時はエラーを表示する', () => {
    mockState = { data: undefined, isLoading: false, isError: true };
    expect(render(true)).toContain('projectUpdates.loadFailed');
  });
});
