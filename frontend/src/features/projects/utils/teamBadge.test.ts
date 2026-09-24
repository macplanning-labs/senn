import { describe, it, expect } from 'vitest';
import { teamBadgeLabelKey } from './teamBadge';

describe('teamBadgeLabelKey', () => {
  it('archived が true のときに teamArchive.badge を返す', () => {
    const result = teamBadgeLabelKey({ archived: true });
    expect(result).toBe('teamArchive.badge');
  });

  it('archived が false のときに null を返す', () => {
    const result = teamBadgeLabelKey({ archived: false });
    expect(result).toBeNull();
  });

  it('archived が undefined のときに null を返す', () => {
    const result = teamBadgeLabelKey({ archived: undefined });
    expect(result).toBeNull();
  });

  it('archived フィールドが無いときに null を返す', () => {
    const result = teamBadgeLabelKey({});
    expect(result).toBeNull();
  });
});
