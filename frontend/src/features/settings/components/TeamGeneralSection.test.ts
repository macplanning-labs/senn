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

vi.mock('@/features/teams/hooks/useTeamAccess', () => ({
  useUpdateTeamAccess: () => ({ mutate: vi.fn(), isPending: false }),
  useLeaveTeam: () => ({ mutate: vi.fn(), isPending: false }),
}));

let mockViewer: { id: number; isSystemAdmin: boolean } | null = { id: 1, isSystemAdmin: false };
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
  // Owner の操作ができるか(サーバーの判定)。ボタンはこの値だけで出し分ける(DEMO-000170)
  viewerCanManageOwners: true,
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

  it('Owner の操作ができる人(サーバーの viewerCanManageOwners)には、アーカイブ・復元・削除が出る', () => {
    const can = { ...team, viewerCanManageOwners: true } as unknown as Team;
    const canArchived = { ...archivedTeam, viewerCanManageOwners: true } as unknown as Team;
    expect(renderTeam(can)).toContain('team-archive-btn');
    expect(renderTeam(can)).toContain('team.dangerZone');
    expect(renderTeam(canArchived)).toContain('team-restore-btn');
  });

  it('Owner の操作ができない人には、ボタンを出さず、できる人を案内する(押して初めて拒否される状態にしない)', () => {
    // 画面は自分で権限を計算しない: メンバー一覧で管理者に見えても、サーバーの値が false なら出さない
    mockViewer = { id: 1, isSystemAdmin: true };
    mockMembers = [{ user: { id: 1 }, role: 'admin' }];
    const cannot = { ...team, viewerCanManageOwners: false } as unknown as Team;
    const cannotArchived = { ...archivedTeam, viewerCanManageOwners: false } as unknown as Team;
    const html = renderTeam(cannot);
    expect(html).not.toContain('team-archive-btn');
    expect(html).toContain('team-archive-admin-only');
    expect(html).not.toContain('team.dangerZone');
    const archivedHtml = renderTeam(cannotArchived);
    expect(archivedHtml).not.toContain('team-restore-btn');
    expect(archivedHtml).toContain('team-archive-admin-only');
  });

  describe('アクセス(公開区分・設定の方針・退出)', () => {
    const renderTeam = (extra: Partial<Team>) =>
      renderToStaticMarkup(
        createElement(MemoryRouter, null, createElement(TeamGeneralSection, { team: { ...team, ...extra } })),
      );

    it('公開区分と設定の方針が、サーバーの値で選ばれている', () => {
      const html = renderTeam({ visibility: 'private', settingsPolicy: 'owners', viewerCanManageOwners: true });
      expect(html).toContain('data-testid="team-access-section"');
      // 属性の順序に頼らず、その input の中に checked があるかを見る
      const inputOf = (id: string) => html.match(new RegExp(`<input[^>]*data-testid="${id}"[^>]*>`))?.[0] ?? '';
      expect(inputOf('team-access-private')).toContain('checked=""');
      expect(inputOf('team-access-public')).not.toContain('checked=""');
      expect(inputOf('team-access-policy-owners')).toContain('checked=""');
    });

    it('Owner の操作ができない人には、変更できない状態で出す(サーバーの viewerCanManageOwners で決める)', () => {
      const html = renderTeam({ visibility: 'public', viewerCanManageOwners: false });
      expect(html).toMatch(/<fieldset[^>]*disabled=""/);
    });

    it('参加しているときだけ、退出のボタンを出す', () => {
      expect(renderTeam({ viewerIsMember: true })).toContain('data-testid="team-leave-btn"');
      expect(renderTeam({ viewerIsMember: false })).not.toContain('data-testid="team-leave-btn"');
    });
  });
});
