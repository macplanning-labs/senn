/**
 * i18n.nav-gantt-dependencies.test.ts — ナビゲーション i18n キー検証
 *
 * ガント・依存関係がチームレベルナビゲーションに追加されたことを
 * i18n キーの存在で検証
 */

import { describe, it, expect } from 'vitest';
import i18n from './i18n';

describe('i18n navigation keys for gantt and dependencies', () => {
  it('should have nav.gantt key in both EN and JA', () => {
    const enResources = (i18n.options.resources?.en?.translation) as Record<string, Record<string, any>>;
    const jaResources = (i18n.options.resources?.ja?.translation) as Record<string, Record<string, any>>;

    // EN キーが存在し、空でないことを確認
    const enKey = enResources?.nav?.gantt;
    expect(enKey).toBeTruthy();
    expect(typeof enKey).toBe('string');

    // JA キーが存在し、空でないことを確認
    const jaKey = jaResources?.nav?.gantt;
    expect(jaKey).toBeTruthy();
    expect(typeof jaKey).toBe('string');
  });

  it('should have nav.dependencies key in both EN and JA', () => {
    const enResources = (i18n.options.resources?.en?.translation) as Record<string, Record<string, any>>;
    const jaResources = (i18n.options.resources?.ja?.translation) as Record<string, Record<string, any>>;

    // EN キーが存在し、空でないことを確認
    const enKey = enResources?.nav?.dependencies;
    expect(enKey).toBeTruthy();
    expect(typeof enKey).toBe('string');

    // JA キーが存在し、空でないことを確認
    const jaKey = jaResources?.nav?.dependencies;
    expect(jaKey).toBeTruthy();
    expect(typeof jaKey).toBe('string');
  });
});
