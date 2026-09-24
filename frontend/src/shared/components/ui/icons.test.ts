import { describe, it, expect } from 'vitest';

// ── ガード: アイコンだけのボタンに、文字の「+」「···」「⋯」を書かない ──
// (共通の IconPlus / IconMoreHorizontal を使う。文字はフォントで高さがずれるため)
const sources = import.meta.glob('/src/**/*.tsx', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

describe('アイコンだけのボタン(ガード)', () => {
  it('JSX の中に、単独の「+」「···」「⋯」だけの行を書かない', () => {
    const offenders: string[] = [];
    for (const [path, src] of Object.entries(sources)) {
      if (path.endsWith('.test.tsx')) continue;
      src.split('\n').forEach((line, i) => {
        if (/^\s*(\+|···|⋯|…)\s*$/.test(line)) offenders.push(`${path}:${i + 1}`);
      });
    }
    expect(offenders).toEqual([]);
  });

  it('このガード自身が、ソースを実際に読めている', () => {
    expect(Object.keys(sources).length).toBeGreaterThan(50);
  });
});
