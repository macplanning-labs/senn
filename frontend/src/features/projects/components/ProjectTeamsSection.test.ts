import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

// テスト環境は node のため、DOM ではなくサーバーレンダリングした HTML を検証する
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) => (params ? `${key}${JSON.stringify(params)}` : key),
  }),
}));
vi.mock('./ProjectTeamsSection.css', () => ({}));

type Team = {
  id: number; name: string; slug: string; icon: string; color: string;
  ticketCount: number; cycleCount: number; removable: boolean; removeBlockedReason: string | null; archived: boolean;
};
let mockState: {
  data?: { canManage: boolean; teams: Team[]; addableTeams: { id: number; name: string; slug: string; icon: string; color: string }[] };
  isLoading: boolean;
  isError: boolean;
};
const mutation = { mutate: vi.fn(), isPending: false };
vi.mock('../hooks/useProjectTeams', () => ({
  useProjectTeams: () => mockState,
  useAddProjectTeam: () => mutation,
  useRemoveProjectTeam: () => mutation,
}));

import { ProjectTeamsSection } from './ProjectTeamsSection';

const team = (over: Partial<Team> = {}): Team => ({
  id: 1, name: 'Aチーム', slug: 'a', icon: '👥', color: '#6366f1',
  ticketCount: 0, cycleCount: 0, removable: true, removeBlockedReason: null, archived: false, ...over,
});
const addable = { id: 9, name: 'Bチーム', slug: 'b', icon: '🚀', color: '#ff0000' };
const render = () => renderToStaticMarkup(createElement(ProjectTeamsSection, { projectId: 10 }));

describe('ProjectTeamsSection', () => {
  beforeEach(() => {
    mockState = { isLoading: false, isError: false, data: { canManage: true, teams: [team()], addableTeams: [addable] } };
  });

  it('参加チームを、チケット・サイクルの件数つきで表示する', () => {
    mockState.data!.teams = [team({ ticketCount: 3, cycleCount: 1 })];
    const html = render();
    expect(html).toContain('Aチーム');
    expect(html).toContain('projectTeams.tickets{&quot;count&quot;:3}');
    expect(html).toContain('projectTeams.cycles{&quot;count&quot;:1}');
  });

  it('変更できる人には、外すボタンと、追加できるチームの選択肢を表示する', () => {
    const html = render();
    expect(html).toContain('project-team-remove-1');
    expect(html).toContain('project-teams-select');
    expect(html).toContain('Bチーム');
    expect(html).not.toContain('project-teams-readonly-hint');
  });

  it('外せないチームには外すボタンを出さず、サーバーが返した理由を表示する', () => {
    mockState.data!.teams = [team({ removable: false, removeBlockedReason: 'このプロジェクトにチケット3件が残っているため、チームを外せません' })];
    const html = render();
    expect(html).not.toContain('project-team-remove-1');
    expect(html).toContain('project-team-blocked-1');
    expect(html).toContain('チケット3件が残っている');
  });

  it('変更権限が無い人には、操作を出さず、案内文だけを表示する', () => {
    mockState.data = {
      canManage: false,
      teams: [team({ removable: false, removeBlockedReason: '出してはいけない理由' })],
      addableTeams: [],
    };
    const html = render();
    expect(html).toContain('project-teams-readonly-hint');
    expect(html).not.toContain('project-team-remove-1');
    expect(html).not.toContain('project-teams-select');
    expect(html).not.toContain('出してはいけない理由');
  });

  it('追加できるチームが無いときは、その旨を表示する(選択欄は出さない)', () => {
    mockState.data!.addableTeams = [];
    const html = render();
    expect(html).toContain('projectTeams.noAddable');
    expect(html).not.toContain('project-teams-select');
  });

  it('読み込み失敗時はエラーを表示する', () => {
    mockState = { isLoading: false, isError: true, data: undefined };
    expect(render()).toContain('projectTeams.loadFailed');
  });

  it('アーカイブ済みのチームには、アーカイブバッジを表示する', () => {
    mockState.data!.teams = [team({ archived: true })];
    const html = render();
    expect(html).toContain('project-team-archived-1');
    expect(html).toContain('teamArchive.badge');
  });

  it('アーカイブ済みでないチームには、アーカイブバッジを表示しない', () => {
    mockState.data!.teams = [team({ archived: false })];
    const html = render();
    expect(html).not.toContain('project-team-archived-1');
  });
});
