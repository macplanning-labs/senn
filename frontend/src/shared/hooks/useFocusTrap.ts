/**
 * useFocusTrap.ts — フォーカストラップとアクセシビリティ向上
 *
 * コンテナ内にフォーカスを閉じ込め、Esc キーでモーダル を閉じ、
 * アンマウント時にトリガー要素へフォーカスを戻す。
 */

import { useEffect, useRef } from 'react';

/**
 * 次にフォーカスすべき要素のインデックスを計算する（純粋関数）
 *
 * @param current - 現在のフォーカス要素のインデックス（-1 なら最初）
 * @param count - フォーカス可能要素の総数
 * @param shift - Shift+Tab の場合は true（逆方向）
 * @returns 次のインデックス（循環）
 */
export function getNextFocusIndex(current: number, count: number, shift: boolean): number {
  if (count === 0) return -1;

  // current が範囲外の場合、初期状態として扱う
  if (current < 0 || current >= count) {
    return shift ? count - 1 : 0;
  }

  if (shift) {
    // Shift+Tab：先頭に達したら末尾に戻る
    return current === 0 ? count - 1 : current - 1;
  }
  // Tab：末尾に達したら先頭に戻る
  return current === count - 1 ? 0 : current + 1;
}

/**
 * コンテナ内のフォーカス可能要素を取得（純粋関数）
 *
 * @param container - 検索対象のコンテナ
 * @returns フォーカス可能な要素の配列
 */
export function getFocusableElements(container: HTMLElement): HTMLElement[] {
  const selector = 'input, button, select, textarea, a[href], [tabindex]:not([tabindex="-1"])';
  const elements = Array.from(container.querySelectorAll<HTMLElement>(selector));
  return elements.filter((el) => {
    // 非表示要素を除外
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden') {
      return false;
    }
    // disabled 要素を除外
    if ((el as HTMLButtonElement | HTMLInputElement).disabled) {
      return false;
    }
    return true;
  });
}

/**
 * コンテナ内にフォーカスを閉じ込める。Esc で onEscape を呼ぶ。
 * アンマウント時にトリガー要素へフォーカスを戻す。
 *
 * @param containerRef - コンテナへの参照
 * @param onEscape - Esc キーで呼ぶコールバック
 */
export function useFocusTrap(
  containerRef: React.RefObject<HTMLElement | null>,
  onEscape: () => void,
): void {
  // 復帰先は「初回レンダー時点」のフォーカス要素を保存する。
  // effect 内で取ると、React の autoFocus が先に働いて入力欄になってしまい、閉じたときにトリガーへ戻れない
  const previousActiveElement = useRef<Element | null>(
    typeof document !== 'undefined' ? document.activeElement : null,
  );
  const onEscapeRef = useRef(onEscape);

  // onEscape が変わったら ref を更新（最新の値を参照するため）
  useEffect(() => {
    onEscapeRef.current = onEscape;
  }, [onEscape]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    // cleanup 用に effect 内でコピーしておく（cleanup 時点で ref の値が変わっていても正しい要素へ戻す）
    const returnTarget = previousActiveElement.current;

    // マウント時の初期フォーカス:
    //  1) `data-autofocus` を付けた要素があれば最優先（ヘッダーの閉じるボタンより入力欄を優先したい場合に使う）
    //  2) 既にコンテナ内にフォーカスがある（React の autoFocus 等）ならそれを維持
    //  3) コンテナ内の最初のフォーカス可能要素、それも無ければコンテナ自体
    const preferred = container.querySelector<HTMLElement>('[data-autofocus]');
    if (preferred) {
      preferred.focus();
    } else if (!container.contains(document.activeElement)) {
      const focusableElements = getFocusableElements(container);
      if (focusableElements.length > 0) {
        const first = focusableElements[0];
        if (first) {
          first.focus();
        } else {
          container.focus();
        }
      } else {
        container.focus();
      }
    }

    // keydown リスナー
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onEscapeRef.current();
        return;
      }

      if (e.key !== 'Tab') return;

      const currentlyFocused = document.activeElement;
      if (!container.contains(currentlyFocused)) {
        // コンテナ外がフォーカスされている場合はスキップ
        return;
      }

      const focusableEls = getFocusableElements(container);
      if (focusableEls.length === 0) {
        // フォーカス可能要素がない場合はコンテナにフォーカスを留める
        e.preventDefault();
        container.focus();
        return;
      }

      const currentIndex = focusableEls.indexOf(currentlyFocused as HTMLElement);
      const nextIndex = getNextFocusIndex(currentIndex, focusableEls.length, e.shiftKey);

      e.preventDefault();
      const next = focusableEls[nextIndex];
      if (next) {
        next.focus();
      }
    };

    document.addEventListener('keydown', handleKeyDown);

    return () => {
      document.removeEventListener('keydown', handleKeyDown);

      // アンマウント時：前のアクティブ要素へフォーカスを戻す
      if (returnTarget && document.contains(returnTarget)) {
        (returnTarget as HTMLElement).focus();
      }
    };
  }, [containerRef]);
}
