/**
 * teamPermissions.ts — チームのアーカイブ・復元を、画面に出すかどうかの判定
 *
 * 最終的な権限の判定は、サーバー側(システム管理者 / そのチームの管理者)。
 * ここは「押しても拒否されるボタンを出さない」ための案内用で、サーバーの規則と揃える。
 */

interface MemberLike {
  user: { id: number };
  role: string;
}

interface ViewerLike {
  id: number;
  isStaff: boolean;
}

/** システム管理者、または、そのチームの管理者(role が admin)か。 */
export function canManageTeam(
  viewer: ViewerLike | null | undefined,
  members: MemberLike[] | null | undefined,
): boolean {
  if (!viewer) return false;
  if (viewer.isStaff) return true;
  return (members ?? []).some((m) => m.user.id === viewer.id && m.role === 'admin');
}
