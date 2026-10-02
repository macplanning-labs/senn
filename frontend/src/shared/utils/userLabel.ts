/**
 * userLabel.ts — 人の名前の表示を、画面をまたいで揃える
 *
 * 表示名（displayName）があればそれ、無ければユーザー名。
 * 画面ごとに別々の決め方（firstName など）をすると、同じ人が場所によって別人に見えてしまう
 * （左下は「h.yamada」、メンバー一覧は「山田花子」）。人の名前を出すときは、必ずここを通す。
 */

type Nameable = { displayName?: string | null; username?: string | null } | null | undefined;

/** 画面に出す名前 */
export function userLabel(user: Nameable, fallback = ''): string {
  return user?.displayName?.trim() || user?.username?.trim() || fallback;
}

/** アバターの頭文字（大文字） */
export function userInitial(user: Nameable, fallback = '?'): string {
  return Array.from(userLabel(user))[0]?.toUpperCase() ?? fallback;
}
