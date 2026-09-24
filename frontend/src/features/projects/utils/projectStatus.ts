/**
 * projectStatus.ts — プロジェクトのステータス(planned / in_progress / paused / completed)の表示
 */

/** ステータスに対応する i18n キー。既知でなければ null(呼び出し側は生の値を出す) */
export function projectStatusLabelKey(status: string): string | null {
  switch (status) {
    case 'planned':
      return 'sidebar.projectStatus.planned';
    case 'in_progress':
      return 'sidebar.projectStatus.inProgress';
    case 'paused':
      return 'sidebar.projectStatus.paused';
    case 'completed':
      return 'sidebar.projectStatus.completed';
    default:
      return null;
  }
}
