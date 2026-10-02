import { describe, expect, it } from 'vitest';
import { discoverableTeams, isJoined, isPrivateTeam, joinedTeams } from './teamAccess';

const t = (o: { visibility?: 'public' | 'private'; viewerIsMember?: boolean; archivedAt?: string | null }) => o;

describe('teamAccess', () => {
  it('項目の無い古い応答は、参加済みとして扱う', () => {
    expect(isJoined(t({}))).toBe(true);
    expect(joinedTeams([t({}), t({ viewerIsMember: false })])).toHaveLength(1);
  });

  it('Private の判定', () => {
    expect(isPrivateTeam(t({ visibility: 'private' }))).toBe(true);
    expect(isPrivateTeam(t({ visibility: 'public' }))).toBe(false);
    expect(isPrivateTeam(t({}))).toBe(false);
  });

  it('探すチームは、Public・未参加・アーカイブされていない物だけ', () => {
    const teams = [
      t({ visibility: 'public', viewerIsMember: false }),
      t({ visibility: 'public', viewerIsMember: true }),
      t({ visibility: 'private', viewerIsMember: false }),
      t({ visibility: 'public', viewerIsMember: false, archivedAt: '2026-01-01' }),
    ];
    expect(discoverableTeams(teams)).toEqual([teams[0]]);
  });
});
