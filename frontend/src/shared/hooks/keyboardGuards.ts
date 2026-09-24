/**
 * 単独キーのショートカット(c / x / j / k / g など)を、Cmd+C や Ctrl+X のような
 * ブラウザ標準の操作(コピー・切り取り・検索など)と取り違えないための判定。
 * Cmd / Ctrl / Alt を押しているときは、ショートカットとして扱わない
 * (Shift は大文字入力なので対象外)。
 */
export function hasCommandModifier(
  e: Pick<KeyboardEvent, 'metaKey' | 'ctrlKey' | 'altKey'>,
): boolean {
  return e.metaKey || e.ctrlKey || e.altKey;
}
