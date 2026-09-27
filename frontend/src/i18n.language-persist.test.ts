/**
 * i18n.language-persist.test.ts - 言語の決め方と保存
 *
 * - 初回はブラウザの言語で決める。日本語以外は英語。
 * - 利用者が切り替えた言語は保存され、次回以降(再読み込み・ログアウト後)も保たれる。
 * - localStorage が使えない環境でも、表示の切替自体は動く。
 */
import { describe, it, expect, vi, afterEach } from 'vitest';
import i18n, { resolveInitialLanguage, changeAppLanguage, LANGUAGE_STORAGE_KEY } from './i18n';

describe('resolveInitialLanguage', () => {
  it('uses the saved language first', () => {
    expect(resolveInitialLanguage('en', 'ja-JP')).toBe('en');
    expect(resolveInitialLanguage('ja', 'en-US')).toBe('ja');
  });

  it('falls back to the browser language when nothing is saved', () => {
    expect(resolveInitialLanguage(null, 'ja-JP')).toBe('ja');
    expect(resolveInitialLanguage(null, 'en-US')).toBe('en');
    expect(resolveInitialLanguage(undefined, 'fr-FR')).toBe('en');
    expect(resolveInitialLanguage(null, undefined)).toBe('en');
  });

  it('ignores an invalid saved value', () => {
    expect(resolveInitialLanguage('xx', 'ja-JP')).toBe('ja');
    expect(resolveInitialLanguage('', 'en-US')).toBe('en');
  });
});

describe('changeAppLanguage', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('switches the language and saves the choice', async () => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) });
    changeAppLanguage('ja');
    await vi.waitFor(() => expect(i18n.language).toBe('ja'));
    expect(store.get(LANGUAGE_STORAGE_KEY)).toBe('ja');
    changeAppLanguage('en');
    await vi.waitFor(() => expect(i18n.language).toBe('en'));
    expect(store.get(LANGUAGE_STORAGE_KEY)).toBe('en');
  });

  it('still switches when localStorage is unavailable', async () => {
    vi.stubGlobal('localStorage', {
      getItem: () => { throw new Error('denied'); },
      setItem: () => { throw new Error('denied'); },
    });
    expect(() => changeAppLanguage('ja')).not.toThrow();
    await vi.waitFor(() => expect(i18n.language).toBe('ja'));
    changeAppLanguage('en');
    await vi.waitFor(() => expect(i18n.language).toBe('en'));
  });
});
