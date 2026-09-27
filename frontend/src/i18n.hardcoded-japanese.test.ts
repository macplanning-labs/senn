/**
 * i18n.hardcoded-japanese.test.ts - ソースに直接書かれた日本語の「これ以上増やさない」ガード
 *
 * 画面に出る文言を、翻訳(i18n.ts)を通さずにソースへ日本語で直接書くと、英語の画面に
 * 日本語が出てしまう。過去の分は段階的に i18n へ移しており、ファイルごとの行数の上限
 * (src/i18n.hardcoded-baseline.json)を、移すたびに下げていく。
 *
 *  - 上限を超えた(日本語の直書きが増えた) → 失敗。文言は t('キー') と i18n.ts に入れる。
 *  - 上限より減った → 失敗(基準を下げ忘れないように)。次で基準を更新する:
 *      UPDATE_I18N_BASELINE=1 npx vitest run src/i18n.hardcoded-japanese.test.ts
 *  - コメントの中の日本語は数えない。
 */
import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const sources = import.meta.glob('./**/*.{ts,tsx}', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
const CJK = /[぀-ヿ㐀-鿿]/;
const BASELINE_PATH = path.resolve(__dirname, 'i18n.hardcoded-baseline.json');

function stripComments(src: string): string {
  const noBlock = src.replace(/\/\*[\s\S]*?\*\//g, '');
  return noBlock
    .split('\n')
    .map((line) => {
      const m = /(?<!:)\/\//.exec(line);
      if (!m) return line;
      const before = line.slice(0, m.index);
      const quotes = (before.match(/['"`]/g) ?? []).length;
      return quotes % 2 === 0 ? before : line; // 文字列の中の // (URL など)は残す
    })
    .join('\n');
}

function currentCounts(): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [file, src] of Object.entries(sources)) {
    if (/\.test\.|generated|i18n\.ts$|demoFixtures|demoData/.test(file)) continue;
    const n = stripComments(src).split('\n').filter((l) => CJK.test(l)).length;
    if (n > 0) out[file.replace('./', 'src/')] = n;
  }
  return Object.fromEntries(Object.entries(out).sort(([a], [b]) => a.localeCompare(b)));
}

describe('hard-coded Japanese in UI source does not grow', () => {
  const counts = currentCounts();

  if (process.env.UPDATE_I18N_BASELINE) {
    it('updates the baseline', () => {
      fs.writeFileSync(BASELINE_PATH, JSON.stringify(counts, null, 2) + '\n');
      expect(true).toBe(true);
    });
    return;
  }

  const baseline = JSON.parse(fs.readFileSync(BASELINE_PATH, 'utf-8')) as Record<string, number>;

  it('has no new file with hard-coded Japanese', () => {
    const added = Object.keys(counts).filter((f) => !(f in baseline));
    expect(added, 'Put new UI text in i18n.ts and use t(); do not hard-code Japanese.').toEqual([]);
  });

  it('has no file whose hard-coded Japanese lines increased', () => {
    const grown = Object.entries(counts).filter(([f, n]) => f in baseline && n > baseline[f]).map(([f, n]) => `${f}: ${baseline[f]} -> ${n}`);
    expect(grown).toEqual([]);
  });

  it('baseline is up to date (lower it when you move text into i18n)', () => {
    const shrunk = Object.entries(baseline).filter(([f, n]) => (counts[f] ?? 0) < n).map(([f, n]) => `${f}: ${n} -> ${counts[f] ?? 0}`);
    expect(shrunk, 'Run: UPDATE_I18N_BASELINE=1 npx vitest run src/i18n.hardcoded-japanese.test.ts').toEqual([]);
  });
});
