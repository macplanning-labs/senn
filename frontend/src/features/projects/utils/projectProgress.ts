/**
 * projectProgress.ts — プロジェクト進捗の表示整形（純関数）
 */

/**
 * 進捗パーセンテージを表示用文字列に変換
 * progress が null の場合は「—」、それ以外は整数パーセント（小数第1位で丸める）
 */
export function formatProgress(progress: number | null): string {
  if (progress === null) return '—';
  return `${Math.round(progress * 100)}%`;
}
