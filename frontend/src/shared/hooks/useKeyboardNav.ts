/**
 * useKeyboardNav.ts — キーボードナビゲーションフック
 *
 * チケット一覧でのキーボード操作:
 *   j / ↓  : 次の行を選択
 *   k / ↑  : 前の行を選択
 *   Enter  : 選択中のチケットを右ペインで開く
 *   Escape : 右ペインを閉じる
 *   c      : 新規チケット作成に遷移
 *   x      : 選択中チケットのステータスを次に進める
 */

import { useEffect, useCallback, useState } from 'react';
import { hasCommandModifier } from './keyboardGuards';

interface UseKeyboardNavOptions {
  /** リストのアイテム数 */
  itemCount: number;
  /** アイテム選択時のコールバック */
  onSelect?: (index: number) => void;
  /** Enter押下時のコールバック */
  onOpen?: (index: number) => void;
  /** Escape押下時のコールバック */
  onClose?: () => void;
  /** 'c' 押下時のコールバック */
  onCreate?: () => void;
  /** 'x' 押下時のコールバック */
  onAdvanceStatus?: (index: number) => void;
  /** キーボードナビを有効にするか */
  enabled?: boolean;
}

/** テキスト入力中かどうかを判定 */
function isInputFocused(): boolean {
  const active = document.activeElement;
  if (!active) return false;
  const tag = active.tagName.toLowerCase();
  return (
    tag === 'input' ||
    tag === 'textarea' ||
    tag === 'select' ||
    (active as HTMLElement).isContentEditable
  );
}

export function useKeyboardNav({
  itemCount,
  onSelect,
  onOpen,
  onClose,
  onCreate,
  onAdvanceStatus,
  enabled = true,
}: UseKeyboardNavOptions) {
  const [selectedIndex, setSelectedIndex] = useState(-1);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Cmd+C(コピー)などを、単独キーのショートカットと取り違えない
      if (!enabled || hasCommandModifier(e) || isInputFocused()) return;

      switch (e.key) {
        case 'j':
        case 'ArrowDown': {
          e.preventDefault();
          const next = Math.min(selectedIndex + 1, itemCount - 1);
          setSelectedIndex(next);
          onSelect?.(next);
          break;
        }
        case 'k':
        case 'ArrowUp': {
          e.preventDefault();
          const prev = Math.max(selectedIndex - 1, 0);
          setSelectedIndex(prev);
          onSelect?.(prev);
          break;
        }
        case 'Enter': {
          if (selectedIndex >= 0) {
            e.preventDefault();
            onOpen?.(selectedIndex);
          }
          break;
        }
        case 'Escape': {
          e.preventDefault();
          onClose?.();
          setSelectedIndex(-1);
          break;
        }
        case 'c': {
          e.preventDefault();
          onCreate?.();
          break;
        }
        case 'x': {
          if (selectedIndex >= 0) {
            e.preventDefault();
            onAdvanceStatus?.(selectedIndex);
          }
          break;
        }
      }
    },
    [enabled, selectedIndex, itemCount, onSelect, onOpen, onClose, onCreate, onAdvanceStatus],
  );

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  return {
    selectedIndex,
    setSelectedIndex,
  };
}
