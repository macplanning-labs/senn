/**
 * useHistoryNav.ts — ヘッダーの「戻る / 進む」（アプリ内の履歴移動）
 *
 * 詳細画面から別の画面を見たあと、元の詳細画面へ戻れるようにする。
 * 「← 一覧名」（BackLink）やパンくずは「決まった親」へ移動するのに対し、
 * こちらは直前に見ていた画面へ戻る。
 *
 * - 履歴の位置は React Router が history.state.idx に入れる番号で判定する
 * - ⌘[ / ⌘]（Ctrl+[ / Ctrl+]）はデスクトップアプリ（Tauri）のときだけ処理する。
 *   ブラウザは同じキーで自前の戻る・進むを行うため、二重に動かさない
 */

import { useCallback, useEffect, useState } from 'react';
import { useLocation, useNavigate, useNavigationType } from 'react-router-dom';

type NavigationType = 'PUSH' | 'REPLACE' | 'POP';

/** 現在の履歴位置（React Router が history.state.idx に入れる番号） */
function currentIndex(): number {
  if (typeof window === 'undefined') return 0;
  const state = window.history.state as { idx?: unknown } | null;
  return typeof state?.idx === 'number' ? state.idx : 0;
}

/** デスクトップアプリ（Tauri v2）で動いているか */
export function isDesktopApp(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * 「進む」で行ける一番先の位置を更新する。
 * 新しい画面へ進んだ（PUSH）ときは、それより先の履歴は無くなる。
 * 置き換え（REPLACE）や戻る・進む（POP）では、先の履歴は残る。
 */
export function nextHistoryMax(prevMax: number, index: number, type: NavigationType): number {
  if (type === 'PUSH') return index;
  return Math.max(prevMax, index);
}

// レイアウトが作り直されても「進む」の範囲を失わないよう、モジュール内で持つ
let maxIndex = 0;

export function useHistoryNav() {
  const navigate = useNavigate();
  const location = useLocation();
  const navigationType = useNavigationType() as NavigationType;
  const [position, setPosition] = useState(() => {
    const index = currentIndex();
    maxIndex = Math.max(maxIndex, index);
    return { index, max: maxIndex };
  });

  useEffect(() => {
    const index = currentIndex();
    maxIndex = nextHistoryMax(maxIndex, index, navigationType);
    setPosition({ index, max: maxIndex });
  }, [location.key, navigationType]);

  const canGoBack = position.index > 0;
  const canGoForward = position.index < position.max;

  const goBack = useCallback(() => {
    if (canGoBack) navigate(-1);
  }, [canGoBack, navigate]);

  const goForward = useCallback(() => {
    if (canGoForward) navigate(1);
  }, [canGoForward, navigate]);

  useEffect(() => {
    if (!isDesktopApp()) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
      if (e.code === 'BracketLeft') {
        e.preventDefault();
        goBack();
      } else if (e.code === 'BracketRight') {
        e.preventDefault();
        goForward();
      }
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [goBack, goForward]);

  return { canGoBack, canGoForward, goBack, goForward };
}
