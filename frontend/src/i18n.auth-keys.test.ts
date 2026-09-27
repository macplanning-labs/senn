/**
 * i18n.auth-keys.test.ts - 認証画面の翻訳キー漏れ防止
 *
 * features/auth の画面が t('auth.xxx', '既定の英文') で使うキーが、EN/JA の両方に
 * 定義されていることを検証する。JA に無いと日本語UIでも既定の英文が表示される
 * (例: ログイン失敗時に "Invalid username or password" が英語のまま出ていた)。
 */

import { describe, it, expect } from 'vitest';
import i18n from './i18n';

const sources = import.meta.glob('./features/auth/**/*.tsx', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

function usedAuthKeys(): string[] {
  const keys = new Set<string>();
  for (const src of Object.values(sources)) {
    for (const m of src.matchAll(/\bt\(\s*'auth\.([A-Za-z0-9_.]+)'/g)) keys.add(m[1]);
  }
  return [...keys].sort();
}

describe('i18n auth keys used by features/auth', () => {
  const en = i18n.options.resources?.en?.translation as Record<string, Record<string, string>>;
  const ja = i18n.options.resources?.ja?.translation as Record<string, Record<string, string>>;

  it('finds the keys used by the auth screens', () => {
    expect(usedAuthKeys().length).toBeGreaterThan(0);
  });

  it.each(usedAuthKeys())('auth.%s exists in both EN and JA', (key) => {
    const path = key.split('.');
    const pick = (res: Record<string, any>) => path.reduce((o, k) => o?.[k], res.auth);
    expect(pick(en), `EN auth.${key}`).toBeTruthy();
    expect(pick(ja), `JA auth.${key}`).toBeTruthy();
  });

  it('shows the login error in Japanese in the JA UI', () => {
    expect(ja.auth.loginError).toMatch(/[ぁ-んァ-ヶ一-龠]/);
    expect(en.auth.loginError).toBe('Invalid username or password');
  });
});
