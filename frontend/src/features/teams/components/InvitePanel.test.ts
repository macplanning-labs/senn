import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

// テスト環境は node のため、DOM ではなくサーバーレンダリングした HTML を検証する
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock('./InvitePanel.css', () => ({}));
vi.mock('@/shared/stores/toastStore', () => ({ useToastStore: () => ({ addToast: vi.fn() }) }));
vi.mock('@/shared/sync/repos/projectRepo', () => ({
  useProjects: () => ({
    projects: [
      { id: 1, name: '営業案件', teams: [{ id: 7 }] },
      { id: 2, name: '他チームの案件', teams: [{ id: 8 }] },
    ],
  }),
}));
let pending: unknown[] = [];
const mutation = { mutate: vi.fn(), isPending: false };
vi.mock('../hooks/useInvitations', () => ({
  useInvitations: () => ({ data: pending }),
  useCreateInvitation: () => mutation,
  useRevokeInvitation: () => mutation,
}));

import { InvitePanel, projectsOfTeam } from './InvitePanel';

const render = (teamId: number | null) => renderToStaticMarkup(createElement(InvitePanel, { teamId }));

describe('InvitePanel', () => {
  beforeEach(() => {
    pending = [];
  });

  it('チームの招待: 種別(Full Member / Guest)を選べ、既定は Full Member(招待した人はチームに入る)', () => {
    const html = render(7);
    expect(html).toContain('invite-role-full');
    expect(html).toContain('invite-role-guest');
    // 既定で選ばれているのは Full Member。Guest の範囲の選択は、Guest を選ぶまで出さない
    const radio = (id: string) => html.match(new RegExp(`<input[^>]*data-testid="${id}"[^>]*>`))?.[0] ?? '';
    expect(radio('invite-role-full')).toContain('checked=""');
    expect(radio('invite-role-guest')).not.toContain('checked');
    expect(html).not.toContain('invite-project');
    expect(html).toContain('invite.hintTeam');
  });

  it('Guest の範囲の候補は、このチームが参加しているプロジェクトだけ', () => {
    const projects = [
      { id: 1, name: '営業案件', teams: [{ id: 7 }] },
      { id: 2, name: '他チームの案件', teams: [{ id: 8 }] },
      { id: 3, name: 'チーム無し', teams: null },
    ];
    expect(projectsOfTeam(projects, 7).map((p) => p.name)).toEqual(['営業案件']);
    expect(projectsOfTeam(projects, null)).toEqual([]);
  });

  it('管理画面(チーム無し): 種別・範囲は出さない(Full Member の招待だけ)', () => {
    const html = render(null);
    expect(html).not.toContain('invite-role-guest');
    expect(html).not.toContain('invite-project');
    expect(html).toContain('invite.hintAdmin');
  });

  it('未使用の招待を一覧し、取り消せる', () => {
    pending = [
      { id: 5, email: 'a@outside.example', role: 'guest', teamId: 7, scopedProjectId: null, endDate: null, expiresAt: '2026-10-08T00:00:00Z', createdAt: '2026-10-01T00:00:00Z' },
    ];
    const html = render(7);
    expect(html).toContain('a@outside.example');
    expect(html).toContain('invite-revoke-5');
  });
});
