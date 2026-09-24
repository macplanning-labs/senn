import { describe, it, expect, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import type { Team } from '@/shared/api/types';

// テスト環境は node のため、DOM ではなくサーバーレンダリングした HTML を検証する
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock('@/features/teams/hooks/useTeams', () => ({
  useUpdateTeam: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useDeleteTeam: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useArchiveTeam: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useUnarchiveTeam: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useTeamMembers: () => ({ data: mockMembers }),
  useCheckTeamArchive: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

let mockViewer: { id: number; isStaff: boolean } | null = { id: 1, isStaff: false };
let mockMembers: { user: { id: number }; role: string }[] | undefined = [{ user: { id: 1 }, role: 'admin' }];
vi.mock('@/shared/stores/authStore', () => ({
  useAuthStore: (sel: (st: { user: typeof mockViewer }) => unknown) => sel({ user: mockViewer }),
}));

vi.mock('@/shared/stores/toastStore', () => ({
  useToastStore: () => ({ addToast: vi.fn() }),
}));

vi.mock('./TeamGeneralSection.css', () => ({}));

import { TeamGeneralSection } from './TeamGeneralSection';

const team = {
  id: 1,
  name: 'Frontend Team',
  slug: 'frontend-team',
  prefix: 'FE',
  description: 'Frontend team',
  icon: '👥',
  color: '#6366f1',
  slackWebhookUrl: '',
  isActive: true,
} as unknown as Team;

function render(): string {
  return renderToStaticMarkup(
    createElement(MemoryRouter, null, createElement(TeamGeneralSection, { team })),
  );
}

describe('TeamGeneralSection', () => {
  it('現在のチーム情報が編集フォームに入っている', () => {
    const html = render();
    expect(html).toContain('value="Frontend Team"');
    expect(html).toContain('value="FE"');
  });

  it('保存ボタンと、危険な操作エリアの削除ボタンが表示される', () => {
    const html = render();
    expect(html).toContain('team.modal.submitUpdate');
    expect(html).toContain('team.dangerZone');
    expect(html).toContain('team.delete');
  });

  it('削除確認ダイアログは、最初は表示されない', () => {
    expect(render()).not.toContain('confirm-delete-team');
  });

  it('未アーカイブ状態ではアーカイブボタンが表示され、復元ボタンはない', () => {
    const html = render();
    expect(html).toContain('data-testid="team-archive-btn"');
    expect(html).not.toContain('data-testid="team-restore-btn"');
    expect(html).toContain('teamArchive.title');
    expect(html).toContain('teamArchive.description');
  });

  it('アーカイブ済みではアーカイブボタンがなく、復元ボタンがある', () => {
    const archivedTeam = {
      ...team,
      archivedAt: '2026-09-19T10:00:00Z',
    } as unknown as Team;
    const html = renderToStaticMarkup(
      createElement(MemoryRouter, null, createElement(TeamGeneralSection, { team: archivedTeam })),
    );
    expect(html).not.toContain('data-testid="team-archive-btn"');
    expect(html).toContain('data-testid="team-restore-btn"');
    expect(html).toContain('teamArchive.archivedBanner');
    expect(html).toContain('teamArchive.archivedOn');
  });

  it('アーカイブ済みではフォームが無効化される', () => {
    const archivedTeam = {
      ...team,
      archivedAt: '2026-09-19T10:00:00Z',
    } as unknown as Team;
    const html = renderToStaticMarkup(
      createElement(MemoryRouter, null, createElement(TeamGeneralSection, { team: archivedTeam })),
    );
    expect(html).toContain('teamArchive.settingsLocked');
    // fieldset に disabled 属性がある
    expect(html).toContain('disabled');
  });

  it('未アーカイブではアーカイブボタンのテストidが存在する', () => {
    const html = render();
    expect(html).toContain('data-testid="team-archive-btn"');
  });

  it('アーカイブ済みでは復元ボタンのテストidが存在する', () => {
    const archivedTeam = {
      ...team,
      archivedAt: '2026-09-19T10:00:00Z',
    } as unknown as Team;
    const html = renderToStaticMarkup(
      createElement(MemoryRouter, null, createElement(TeamGeneralSection, { team: archivedTeam })),
    );
    expect(html).toContain('data-testid="team-restore-btn"');
  });

  it('初期表示（SSR）では、アーカイブできない理由パネルが出ていない', () => {
    const html = render();
    expect(html).not.toContain('data-testid="team-archive-blocked"');
  });
});

describe('TeamGeneralSection: アーカイブ・復元の権限別の表示', () => {
  const archivedTeam = { ...team, archivedAt: '2026-09-19T10:00:00Z' } as unknown as Team;
  const renderTeam = (t: Team) =>
    renderToStaticMarkup(createElement(MemoryRouter, null, createElement(TeamGeneralSection, { team: t })));

  it('チームの管理者には、アーカイブ・復元のボタンが出る', () => {
    mockViewer = { id: 1, isStaff: false };
    mockMembers = [{ user: { id: 1 }, role: 'admin' }];
    expect(renderTeam(team)).toContain('team-archive-btn');
    expect(renderTeam(archivedTeam)).toContain('team-restore-btn');
  });

  it('システム管理者には、そのチームに所属していなくても、ボタンが出る', () => {
    mockViewer = { id: 99, isStaff: true };
    mockMembers = [];
    expect(renderTeam(team)).toContain('team-archive-btn');
  });

  it('管理者ではない人には、ボタンを出さず、できる人を案内する(押して初めて拒否される状態にしない)', () => {
    for (const viewer of [{ id: 2, isStaff: false }, { id: 3, isStaff: false }]) {
      mockViewer = viewer;
      mockMembers = [{ user: { id: 1 }, role: 'admin' }, { user: { id: 2 }, role: 'member' }];
      const html = renderTeam(team);
      expect(html).not.toContain('team-archive-btn');
      expect(html).toContain('team-archive-admin-only');
      const archivedHtml = renderTeam(archivedTeam);
      expect(archivedHtml).not.toContain('team-restore-btn');
      expect(archivedHtml).toContain('team-archive-admin-only');
    }
  });
});
