import { describe, it, expect } from 'vitest';
import { hasCommandModifier } from './keyboardGuards';

const key = (o: Partial<Record<'metaKey' | 'ctrlKey' | 'altKey', boolean>> = {}) => ({
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  ...o,
});

describe('hasCommandModifier', () => {
  it('修飾キー無しはショートカットとして扱う', () => {
    expect(hasCommandModifier(key())).toBe(false);
  });

  it('Cmd(Mac)/Ctrl(Windows)/Alt を押していれば true(コピー・切り取り等を邪魔しない)', () => {
    expect(hasCommandModifier(key({ metaKey: true }))).toBe(true);
    expect(hasCommandModifier(key({ ctrlKey: true }))).toBe(true);
    expect(hasCommandModifier(key({ altKey: true }))).toBe(true);
  });
});
