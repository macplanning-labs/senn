import { describe, it, expect } from 'vitest';
import { buildCopyPrompt, UserAiPromptTemplates } from './buildCopyPrompt';

describe('buildCopyPrompt', () => {
  const defaults = {
    common: 'default common rules',
    cursor: 'default cursor rules',
    claude: 'default claude rules',
  };

  it('個人設定が未取得のとき、既定値で動く', () => {
    const cached = 'cached prompt text';
    const result = buildCopyPrompt(cached, 'Cursor', undefined, defaults);
    expect(result).toBe('cached prompt text\n\n---\ndefault common rules\ndefault cursor rules');
  });

  it('キャッシュが空の場合、空文字列を返す', () => {
    const cached = '';
    const result = buildCopyPrompt(cached, 'Cursor', undefined, defaults);
    expect(result).toBe('');
  });

  it('キャッシュがnullの場合、空文字列を返す', () => {
    const cached = null;
    const result = buildCopyPrompt(cached, 'Cursor', undefined, defaults);
    expect(result).toBe('');
  });

  it('キャッシュが undefined の場合、空文字列を返す', () => {
    const cached = undefined;
    const result = buildCopyPrompt(cached, 'Cursor', undefined, defaults);
    expect(result).toBe('');
  });

  it('個人設定が未取得(undefined)のとき、既定値で動く', () => {
    const cached = 'cached prompt text';
    const result = buildCopyPrompt(cached, 'Cursor', undefined, defaults);
    expect(result).toBe('cached prompt text\n\n---\ndefault common rules\ndefault cursor rules');
  });

  it('個人設定が未設定(null値)のとき、既定値で動く', () => {
    const cached = 'cached prompt text';
    const userTemplates: UserAiPromptTemplates = { common: null, cursor: null, claude: null };
    const result = buildCopyPrompt(cached, 'Cursor', userTemplates, defaults);
    expect(result).toBe('cached prompt text\n\n---\ndefault common rules\ndefault cursor rules');
  });

  it('個人設定の共通テンプレートのみある場合、それと既定値のツール用を結合', () => {
    const cached = 'cached prompt text';
    const userTemplates: UserAiPromptTemplates = { common: 'my common rules', cursor: null, claude: null };
    const result = buildCopyPrompt(cached, 'Cursor', userTemplates, defaults);
    expect(result).toBe('cached prompt text\n\n---\nmy common rules\ndefault cursor rules');
  });

  it('Cursor用個人設定がある場合、共通 + Cursor用を結合', () => {
    const cached = 'cached prompt text';
    const userTemplates: UserAiPromptTemplates = { common: null, cursor: 'my cursor rules', claude: null };
    const result = buildCopyPrompt(cached, 'Cursor', userTemplates, defaults);
    expect(result).toBe('cached prompt text\n\n---\ndefault common rules\nmy cursor rules');
  });

  it('Claude Code用個人設定がある場合、共通 + Claude Code用を結合', () => {
    const cached = 'cached prompt text';
    const userTemplates: UserAiPromptTemplates = { common: null, cursor: null, claude: 'my claude rules' };
    const result = buildCopyPrompt(cached, 'Claude Code', userTemplates, defaults);
    expect(result).toBe('cached prompt text\n\n---\ndefault common rules\nmy claude rules');
  });

  it('個人設定の共通・ツール用両方ある場合、それらを結合', () => {
    const cached = 'cached prompt text';
    const userTemplates: UserAiPromptTemplates = { common: 'my common', cursor: 'my cursor', claude: null };
    const result = buildCopyPrompt(cached, 'Cursor', userTemplates, defaults);
    expect(result).toBe('cached prompt text\n\n---\nmy common\nmy cursor');
  });

  it('前後の空白を削除して結合する', () => {
    const cached = '  cached prompt text  ';
    const userTemplates: UserAiPromptTemplates = { common: '  my common  ', cursor: '  my cursor  ', claude: null };
    const result = buildCopyPrompt(cached, 'Cursor', userTemplates, defaults);
    expect(result).toBe('cached prompt text\n\n---\nmy common\nmy cursor');
  });

  it('共通テンプレートが空白のみ、ツール用のみある場合、ツール用のみを結合', () => {
    const cached = 'cached prompt text';
    const userTemplates: UserAiPromptTemplates = { common: '   ', cursor: 'my cursor', claude: null };
    const result = buildCopyPrompt(cached, 'Cursor', userTemplates, defaults);
    expect(result).toBe('cached prompt text\n\n---\nmy cursor');
  });

  it('共通・ツール用両方が空白のみの場合、キャッシュのみを返す', () => {
    const cached = 'cached prompt text';
    const userTemplates: UserAiPromptTemplates = { common: '  ', cursor: '   ', claude: null };
    const result = buildCopyPrompt(cached, 'Cursor', userTemplates, defaults);
    expect(result).toBe('cached prompt text');
  });

  it('Claude Codeツールの場合、Claude Code用テンプレートを使う', () => {
    const cached = 'cached prompt text';
    const userTemplates: UserAiPromptTemplates = { common: null, cursor: 'cursor rules', claude: 'claude rules' };
    const result = buildCopyPrompt(cached, 'Claude Code', userTemplates, defaults);
    expect(result).toBe('cached prompt text\n\n---\ndefault common rules\nclaude rules');
  });
});
