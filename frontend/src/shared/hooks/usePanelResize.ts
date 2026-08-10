/**
 * usePanelResize.ts — 2ペインレイアウトの境界線ドラッグリサイズ
 *
 * useColumnResize.ts と同様のドラッグ機構。単一の幅(px)を管理し、
 * localStorage に永続化する。
 */

import { useState, useCallback, useRef, useEffect } from 'react';

const STORAGE_PREFIX = 'panel-width-';

interface ResizeState {
  startX: number;
  startWidth: number;
}

/**
 * @param storageKey - localStorage保存キー
 * @param defaultWidth - 初期幅(px)
 * @param minWidth - 最小幅(px)
 * @param maxWidth - 最大幅(px)
 */
export function usePanelResize(
  storageKey: string,
  defaultWidth: number,
  minWidth = 280,
  maxWidth = 800,
) {
  const [width, setWidth] = useState<number>(() => {
    try {
      const stored = localStorage.getItem(STORAGE_PREFIX + storageKey);
      if (stored) {
        const parsed = Number(stored);
        if (!Number.isNaN(parsed)) return Math.min(maxWidth, Math.max(minWidth, parsed));
      }
    } catch { /* ignore */ }
    return defaultWidth;
  });

  const resizeRef = useRef<ResizeState | null>(null);
  const [isResizing, setIsResizing] = useState(false);

  // ドラッグは右→左方向(パネルは画面右側)に伸縮するため、マウス移動量を反転して幅に加算
  const onResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    resizeRef.current = { startX: e.clientX, startWidth: width };
    setIsResizing(true);
  }, [width]);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      const state = resizeRef.current;
      if (!state) return;
      const delta = state.startX - e.clientX;
      const newWidth = Math.min(maxWidth, Math.max(minWidth, state.startWidth + delta));
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      if (resizeRef.current) {
        resizeRef.current = null;
        setIsResizing(false);
        setWidth((current) => {
          try {
            localStorage.setItem(STORAGE_PREFIX + storageKey, String(current));
          } catch { /* ignore */ }
          return current;
        });
      }
    };

    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      document.body.style.userSelect = 'none';
      document.body.style.cursor = 'col-resize';
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.userSelect = '';
      document.body.style.cursor = '';
    };
  }, [isResizing, storageKey, minWidth, maxWidth]);

  return { width, onResizeStart, isResizing };
}
