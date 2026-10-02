import { describe, it, expect, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Team } from '@/shared/api/types';

// テスト環境は node のため、DOM ではなくサーバーレンダリングした HTML を検証する
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const members = [
  { id: 1, user: { id: 1, username: 'owner', displayName: 'Owner さん' }, role: 'admin' },
  { id: 2, user: { id: 2, username: 'member', displayName: 'メンバーさん' }, role: 'member' },
];
vi.mock('../hooks/useTeams', () => ({
  useTeamMembers: () => ({ data: members, isLoading: false }),
  useAddTeamMember: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRemoveTeamMember: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));
vi.mock('../hooks/useTeamAccess', () => ({
  useSetTeamOwner: () => ({ mutate: vi.fn(), isPending: false }),
}));
vi.mock('@tanstack/react-query', () => ({
  useQuery: () => ({ data: [] }),
}));
vi.mock('@/shared/api/client', () => ({ apiClient: { get: vi.fn() } }));
vi.mock('@/shared/stores/toastStore', () => ({
  useToastStore: () => ({ addToast: vi.fn() }),
}));
vi.mock('./InvitePanel', () => ({
  InvitePanel: () => createElement('div', { 'data-testid': 'invite-panel' }),
}));

import { TeamMembersSection } from './TeamMembersSection';

const team = { id: 7, name: 'T', icon: '👥', color: '#000' } as unknown as Team;
const render = (extra: Partial<Team>) =>
  renderToStaticMarkup(createElement(TeamMembersSection, { team: { ...team, ...extra } as Team }));

describe('TeamMembersSection: Owner の操作は、サーバーの viewerCanManageOwners だけで出し分ける(DEMO-000170)', () => {
  it('Owner の操作ができる人には、Owner の指名・解除、Owner を外す ✕、招待の欄が出る', () => {
    const html = render({ viewerCanManageOwners: true });
    expect(html).toContain('data-testid="toggle-owner-1"');
    expect(html).toContain('data-testid="toggle-owner-2"');
    expect(html).toContain('data-testid="remove-member-1"');
    expect(html).toContain('data-testid="invite-panel"');
  });

  it('できない人には、Owner の指名・解除、Owner を外す ✕、招待の欄を出さない(押して初めて拒否される状態にしない)', () => {
    // 古い値(viewerCanManage)が true でも、Owner の操作は出さない
    const html = render({ viewerCanManageOwners: false, viewerCanManage: true });
    expect(html).not.toContain('toggle-owner-');
    expect(html).not.toContain('data-testid="remove-member-1"');
    expect(html).not.toContain('data-testid="invite-panel"');
    // Owner でないメンバーの ✕ は、メンバーの管理(Owner の操作ではない)なので残る
    expect(html).toContain('data-testid="remove-member-2"');
  });

  it('アーカイブ済みのチームでは、Owner の操作を出さない', () => {
    const html = render({ viewerCanManageOwners: true, archivedAt: '2026-09-19T10:00:00Z' });
    expect(html).not.toContain('toggle-owner-');
    expect(html).not.toContain('data-testid="invite-panel"');
  });
});
