/**
 * i18n.keys-coverage.test.ts - 翻訳キーの漏れ防止(全画面)
 *
 * t('キー', '既定の文') で使われているキーが、EN/JA の両方に定義されていることを検証する。
 * 定義が無いと、既定の文(多くは日本語)が、そのまま他言語の画面に出てしまう
 * (例: 英語の画面に「プロフィール」「通知」などの日本語が出ていた)。
 */
import { describe, it, expect } from 'vitest';
import i18n from './i18n';

const sources = import.meta.glob('./**/*.{ts,tsx}', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

function usedKeys(): Map<string, string> {
  const keys = new Map<string, string>();
  const re = /\bt\(\s*'([A-Za-z0-9_.]+)'\s*,\s*'/g;
  for (const [file, src] of Object.entries(sources)) {
    if (/\.test\.|generated|i18n\.ts$/.test(file)) continue;
    for (const m of src.matchAll(re)) if (!keys.has(m[1])) keys.set(m[1], file);
  }
  return keys;
}

describe('t() keys with a default text exist in both EN and JA', () => {
  const en = i18n.options.resources?.en?.translation as Record<string, unknown>;
  const ja = i18n.options.resources?.ja?.translation as Record<string, unknown>;
  const pick = (res: Record<string, unknown>, key: string) =>
    key.split('.').reduce<unknown>((o, k) => (o as Record<string, unknown> | undefined)?.[k], res);

  it('finds keys to check', () => {
    expect(usedKeys().size).toBeGreaterThan(50);
  });

  it('has no key missing from EN or JA', () => {
    const missing: string[] = [];
    for (const [key, file] of usedKeys()) {
      if (typeof pick(en, key) !== 'string') missing.push(`EN ${key} (${file})`);
      if (typeof pick(ja, key) !== 'string') missing.push(`JA ${key} (${file})`);
    }
    expect(missing).toEqual([]);
  });
});
