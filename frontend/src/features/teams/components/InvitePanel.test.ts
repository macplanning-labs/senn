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

import { InvitePanel } from './InvitePanel';

const render = (teamId: number | null) => renderToStaticMarkup(createElement(InvitePanel, { teamId }));

describe('InvitePanel', () => {
  beforeEach(() => {
    pending = [];
  });

  it('チームの招待: 種別(Full Member / Guest)を選べ、Guest の範囲はこのチームのプロジェクトだけ', () => {
    const html = render(7);
    expect(html).toContain('invite-role-full');
    expect(html).toContain('invite-role-guest');
    expect(html).toContain('営業案件');
    expect(html).not.toContain('他チームの案件');
    expect(html).toContain('invite.hintTeam');
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
