/**
 * teamBadge.ts — プロジェクト概要でのチーム情報表示ユーティリティ
 */

/**
 * チーム情報から、バッジを表示すべき i18n キーを返す
 * @param team - チーム情報（archived フィールドを含む）
 * @returns バッジの i18n キー。表示すべきバッジがなければ null
 */
export function teamBadgeLabelKey(team: { archived?: boolean }): string | null {
  if (team.archived) {
    return 'teamArchive.badge';
  }
  return null;
}
