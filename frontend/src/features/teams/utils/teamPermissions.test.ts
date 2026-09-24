import { describe, it, expect } from 'vitest';
import { canManageTeam } from './teamPermissions';

const members = [
  { user: { id: 1 }, role: 'admin' },
  { user: { id: 2 }, role: 'member' },
];

describe('canManageTeam', () => {
  it('システム管理者は、チームに所属していなくても、できる', () => {
    expect(canManageTeam({ id: 99, isStaff: true }, members)).toBe(true);
  });

  it('そのチームの管理者はできる', () => {
    expect(canManageTeam({ id: 1, isStaff: false }, members)).toBe(true);
  });

  it('チームの一般メンバーと、所属していない人はできない', () => {
    expect(canManageTeam({ id: 2, isStaff: false }, members)).toBe(false);
    expect(canManageTeam({ id: 3, isStaff: false }, members)).toBe(false);
  });

  it('未ログイン・メンバー未取得のときは、できない(押せるボタンを先に出さない)', () => {
    expect(canManageTeam(null, members)).toBe(false);
    expect(canManageTeam({ id: 1, isStaff: false }, undefined)).toBe(false);
    expect(canManageTeam({ id: 1, isStaff: false }, [])).toBe(false);
  });
});
