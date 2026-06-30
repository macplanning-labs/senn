/**
 * useColumnResize.ts — テーブルカラム幅リサイズフック
 *
 * ドラッグハンドルでカラム幅を調整。
 * 幅は localStorage に永続化し、次回アクセス時に復元。
 */

import { useState, useCallback, useRef, useEffect } from 'react';

export interface ColumnDef {
  key: string;
  /** 初期幅（px） */
  defaultWidth: number;
  /** 最小幅（px） */
  minWidth?: number;
}

interface ResizeState {
  columnKey: string;
  startX: number;
  startWidth: number;
}

const STORAGE_PREFIX = 'col-widths-';

/**
 * テーブルカラム幅のリサイズを管理するフック
 *
 * @param storageKey - localStorage に保存する際のキー
 * @param columns - カラム定義配列
 * @returns { widths, onResizeStart, isResizing }
 */
export function useColumnResize(storageKey: string, columns: ColumnDef[]) {
  // デフォルト幅を構築
  const defaultWidths = columns.reduce<Record<string, number>>((acc, col) => {
    acc[col.key] = col.defaultWidth;
    return acc;
  }, {});

  // localStorage から復元
  const [widths, setWidths] = useState<Record<string, number>>(() => {
    try {
      const stored = localStorage.getItem(STORAGE_PREFIX + storageKey);
      if (stored) {
        const parsed = JSON.parse(stored) as Record<string, number>;
        // 既存カラムのみ復元（カラム構造変更に対応）
        return { ...defaultWidths, ...parsed };
      }
    } catch { /* ignore */ }
    return defaultWidths;
  });

  const resizeRef = useRef<ResizeState | null>(null);
  const [isResizing, setIsResizing] = useState(false);

  // リサイズ開始
  const onResizeStart = useCallback(
    (columnKey: string, e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      resizeRef.current = {
        columnKey,
        startX: e.clientX,
        startWidth: widths[columnKey] ?? defaultWidths[columnKey] ?? 100,
      };
      setIsResizing(true);
    },
    [widths, defaultWidths],
  );

  // マウス移動 & マウスアップ（document レベル）
  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      const state = resizeRef.current;
      if (!state) return;

      const col = columns.find((c) => c.key === state.columnKey);
      const minWidth = col?.minWidth ?? 40;
      const delta = e.clientX - state.startX;
      const newWidth = Math.max(minWidth, state.startWidth + delta);

      setWidths((prev) => ({ ...prev, [state.columnKey]: newWidth }));
    };

    const handleMouseUp = () => {
      if (resizeRef.current) {
        resizeRef.current = null;
        setIsResizing(false);
        // 永続化
        setWidths((current) => {
          try {
            localStorage.setItem(
              STORAGE_PREFIX + storageKey,
              JSON.stringify(current),
            );
          } catch { /* ignore */ }
          return current;
        });
      }
    };

    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      // リサイズ中はテキスト選択を防止
      document.body.style.userSelect = 'none';
      document.body.style.cursor = 'col-resize';
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.userSelect = '';
      document.body.style.cursor = '';
    };
  }, [isResizing, columns, storageKey]);

  return { widths, onResizeStart, isResizing };
}
