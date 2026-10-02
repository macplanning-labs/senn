import { describe, expect, it } from 'vitest';
import { userInitial, userLabel } from './userLabel';

describe('userLabel', () => {
  it('表示名があれば表示名（メンバー一覧・コメントと同じ規則）', () => {
    expect(userLabel({ displayName: '山田花子', username: 'h.yamada' })).toBe('山田花子');
  });
  it('表示名が空・空白ならユーザー名', () => {
    expect(userLabel({ displayName: '', username: 'test.local' })).toBe('test.local');
    expect(userLabel({ displayName: '   ', username: 'test.local' })).toBe('test.local');
    expect(userLabel({ username: 'ai_agent' })).toBe('ai_agent');
    expect(userLabel({ displayName: null, username: 'x' })).toBe('x');
  });
  it('どちらも無ければ fallback', () => {
    expect(userLabel(null, '不明')).toBe('不明');
    expect(userLabel(undefined)).toBe('');
    expect(userLabel({ displayName: '', username: '' }, '?')).toBe('?');
  });
});

describe('userInitial', () => {
  it('名前の先頭1文字（英字は大文字）。日本語もそのまま', () => {
    expect(userInitial({ displayName: '山田花子', username: 'y' })).toBe('山');
    expect(userInitial({ username: 'test.local' })).toBe('T');
  });
  it('絵文字などサロゲートペアの先頭も壊さない', () => {
    expect(userInitial({ displayName: '🎉パーティ', username: 'p' })).toBe('🎉');
  });
  it('名前が無ければ ?', () => {
    expect(userInitial(null)).toBe('?');
    expect(userInitial({ displayName: '', username: '' })).toBe('?');
  });
});
