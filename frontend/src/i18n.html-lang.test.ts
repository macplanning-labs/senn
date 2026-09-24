import { describe, it, expect } from 'vitest';
import { htmlLangFor } from './i18n';

describe('htmlLangFor', () => {
  it('日本語系は ja、それ以外と未指定は en にする', () => {
    expect(htmlLangFor('ja')).toBe('ja');
    expect(htmlLangFor('ja-JP')).toBe('ja');
    expect(htmlLangFor('en')).toBe('en');
    expect(htmlLangFor('en-US')).toBe('en');
    expect(htmlLangFor('fr')).toBe('en');
    expect(htmlLangFor(undefined)).toBe('en');
  });
});
