import { describe, it, expect } from 'vitest';
import { buildTeamsPopoverGroups } from './teamsPopoverGroups';

describe('buildTeamsPopoverGroups', () => {
  const mockTeams = [
    { id: 1, name: 'Zulu Team', slug: 'zulu', icon: '🏠', color: '#FF0000' },
    { id: 2, name: 'Alpha Team', slug: 'alpha', icon: '🎯', color: '#00FF00' },
    { id: 3, name: 'Beta Team', slug: 'beta', icon: '📱', color: '#0000FF', archivedAt: '2024-01-01' },
  ];

  it('separates active and archived teams', () => {
    const result = buildTeamsPopoverGroups(mockTeams);
    expect(result.active).toHaveLength(2);
    expect(result.archived).toHaveLength(1);
    expect(result.active.map((t) => t.slug)).toEqual(['alpha', 'zulu']);
    expect(result.archived.map((t) => t.slug)).toEqual(['beta']);
  });

  it('sorts teams by name in Japanese locale', () => {
    const result = buildTeamsPopoverGroups(mockTeams);
    expect(result.active[0]!.name).toBe('Alpha Team');
    expect(result.active[1]!.name).toBe('Zulu Team');
  });

  it('returns empty arrays for null input', () => {
    const result = buildTeamsPopoverGroups(null);
    expect(result.active).toEqual([]);
    expect(result.archived).toEqual([]);
  });

  it('returns empty arrays for undefined input', () => {
    const result = buildTeamsPopoverGroups(undefined);
    expect(result.active).toEqual([]);
    expect(result.archived).toEqual([]);
  });

  it('returns empty arrays for empty array input', () => {
    const result = buildTeamsPopoverGroups([]);
    expect(result.active).toEqual([]);
    expect(result.archived).toEqual([]);
  });

  it('does not mutate the input array order', () => {
    const originalOrder = mockTeams.map((t) => t.slug);
    buildTeamsPopoverGroups(mockTeams);
    const afterOrder = mockTeams.map((t) => t.slug);
    expect(afterOrder).toEqual(originalOrder);
  });
});
