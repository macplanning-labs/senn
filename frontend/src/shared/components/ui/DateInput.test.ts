import { describe, expect, it } from 'vitest';
import { toLocalIsoDate } from './DateInput';

describe('toLocalIsoDate', () => {
  it('keeps the local calendar day', () => {
    const localMidnight = new Date(2026, 8, 28, 0, 0, 0);
    expect(toLocalIsoDate(localMidnight)).toBe('2026-09-28');
    if (localMidnight.getTimezoneOffset() !== 0) {
      expect(localMidnight.toISOString().slice(0, 10)).not.toBe('2026-09-28');
    }
  });
});
