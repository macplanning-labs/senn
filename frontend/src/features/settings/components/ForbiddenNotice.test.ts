import { describe, it, expect } from 'vitest';
import { isForbiddenError } from './ForbiddenNotice';

describe('isForbiddenError', () => {
  it('403 だけを権限なしとみなす', () => {
    expect(isForbiddenError({ response: { status: 403 } })).toBe(true);
    expect(isForbiddenError({ response: { status: 500 } })).toBe(false);
    expect(isForbiddenError({ response: { status: 401 } })).toBe(false);
    expect(isForbiddenError(new Error('x'))).toBe(false);
    expect(isForbiddenError(null)).toBe(false);
  });
});
