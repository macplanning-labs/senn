/**
 * LanguageToggle.test.ts - 言語切替ボタンの表示
 * 英語の画面では日本語へ、日本語の画面では英語への切替を案内し、ボタン名に日本語が混ざらない。
 */
import { describe, it, expect, afterEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/i18n';
import { LanguageToggle } from './LanguageToggle';

function html(lng: 'en' | 'ja', className?: string): string {
  void i18n.changeLanguage(lng);
  return renderToStaticMarkup(createElement(I18nextProvider, { i18n }, createElement(LanguageToggle, { className })));
}

describe('LanguageToggle', () => {
  afterEach(() => {
    void i18n.changeLanguage('en');
  });

  it('offers Japanese in the English UI, with no Japanese characters in the label', () => {
    const out = html('en');
    expect(out).toContain('title="Switch to Japanese"');
    expect(out).toContain('aria-label="Switch to Japanese"');
    expect(out).not.toMatch(/[぀-ヿ㐀-鿿]/);
  });

  it('offers English in the Japanese UI', () => {
    const out = html('ja');
    expect(out).toContain('title="英語に切替"');
  });

  it('applies the extra class name', () => {
    expect(html('en', 'lang-toggle--floating')).toContain('lang-toggle lang-toggle--floating');
  });
});
