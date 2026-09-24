import { describe, it, expect, afterEach } from 'vitest';
import { nextHistoryMax, isDesktopApp } from './useHistoryNav';

describe('nextHistoryMax(「進む」で行ける範囲)', () => {
  it('新しい画面へ進むと、それより先の履歴は無くなる', () => {
    expect(nextHistoryMax(5, 2, 'PUSH')).toBe(2);
  });

  it('戻る・進む（POP）では、先の履歴が残る', () => {
    expect(nextHistoryMax(5, 2, 'POP')).toBe(5);
  });

  it('置き換え（REPLACE）でも、先の履歴が残る', () => {
    expect(nextHistoryMax(5, 3, 'REPLACE')).toBe(5);
  });

  it('初めての位置は、その位置が範囲になる', () => {
    expect(nextHistoryMax(0, 1, 'PUSH')).toBe(1);
  });
});

describe('isDesktopApp', () => {
  const w = globalThis as unknown as { window?: Record<string, unknown> };
  const original = w.window;
  afterEach(() => {
    w.window = original;
  });

  it('Tauri の目印が無ければブラウザとみなす（⌘[ はブラウザに任せる）', () => {
    w.window = {};
    expect(isDesktopApp()).toBe(false);
  });

  it('Tauri の目印があればデスクトップアプリ', () => {
    w.window = { __TAURI_INTERNALS__: {} };
    expect(isDesktopApp()).toBe(true);
  });
});
