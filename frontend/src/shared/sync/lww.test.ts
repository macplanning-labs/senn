/**
 * lww.test.ts — Last Write Wins 競合解決テスト（詳細設計 §4.1）
 */

import { describe, it, expect } from 'vitest';
import { shouldApply, type LocalVersioned } from './lww';

describe('shouldApply（LWW 競合解決）', () => {
  // テストケース: [説明, local, incomingV, tomb, 期待値]
  it.each<[string, LocalVersioned | undefined, number, { v: number } | undefined, 'apply' | 'skip']>([
    // ── ローカル行が存在しない場合 ──
    ['no local, no tomb → apply', undefined, 1, undefined, 'apply'],
    ['no local, tomb.v >= incoming → skip', undefined, 1, { v: 2 }, 'skip'],
    ['no local, tomb.v >= incoming (equal) → skip', undefined, 1, { v: 1 }, 'skip'],
    ['no local, tomb.v < incoming → apply', undefined, 2, { v: 1 }, 'apply'],

    // ── ローカル行が存在し、未送信フラグなし ──
    ['local.v lower → apply', { v: 0 }, 1, undefined, 'apply'],
    ['local.v equal → skip', { v: 1 }, 1, undefined, 'skip'],
    ['local.v higher → skip', { v: 2 }, 1, undefined, 'skip'],

    // ── ローカル v が undefined（作成中など）──
    ['local.v undefined (treated as 0), incoming 1 → apply', { v: undefined }, 1, undefined, 'apply'],
    ['local.v undefined (treated as 0), incoming 0 → skip', { v: undefined }, 0, undefined, 'skip'],

    // ── 未送信フラグがある場合はバージョンに関わらず skip ──
    ['_dirty=true, much higher incoming → skip', { v: 1, _dirty: true }, 100, undefined, 'skip'],
    ['_pendingCreate=true, much higher incoming → skip', { v: 1, _pendingCreate: true }, 100, undefined, 'skip'],
    ['_deleted=true, much higher incoming → skip', { v: 1, _deleted: true }, 100, undefined, 'skip'],

    // ── フラグが false/undefined の場合は正常に動作 ──
    ['_dirty=false, normal version compare → apply', { v: 0, _dirty: false }, 1, undefined, 'apply'],
    ['_dirty=undefined, normal version compare → apply', { v: 0 }, 1, undefined, 'apply'],
  ])('%s', (_, local, incomingV, tomb, expected) => {
    expect(shouldApply(local, incomingV, tomb)).toBe(expected);
  });

  // ── edge cases ──
  it('複数フラグが true でも skip', () => {
    const local: LocalVersioned = { v: 0, _dirty: true, _pendingCreate: true };
    expect(shouldApply(local, 100, undefined)).toBe('skip');
  });

  it('tomb と local の両方がある場合、local の未送信フラグが優先', () => {
    const local: LocalVersioned = { v: 0, _dirty: true };
    expect(shouldApply(local, 5, { v: 10 })).toBe('skip');
  });

  it('local が undefined で tomb.v === incomingV のときも skip', () => {
    expect(shouldApply(undefined, 5, { v: 5 })).toBe('skip');
  });
});
