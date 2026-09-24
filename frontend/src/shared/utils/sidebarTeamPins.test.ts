import { describe, it, expect, beforeEach } from 'vitest';
import {
  readPinnedSlugs,
  togglePinnedSlug,
  splitPinnedTeams,
  SIDEBAR_TEAM_PINS_KEY,
  isPinnedSlug,
} from './sidebarTeamPins';

function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (k) => map.get(k) ?? null,
    setItem: (k, v) => {
      map.set(k, v);
    },
    removeItem: (k) => {
      map.delete(k);
    },
    key: (i) => [...map.keys()][i] ?? null,
  };
}

describe('sidebarTeamPins', () => {
  let storage: Storage;

  beforeEach(() => {
    storage = memoryStorage();
  });

  it('readPinnedSlugs returns [] for empty/invalid', () => {
    expect(readPinnedSlugs(storage)).toEqual([]);
    storage.setItem(SIDEBAR_TEAM_PINS_KEY, 'not-json');
    expect(readPinnedSlugs(storage)).toEqual([]);
  });

  it('togglePinnedSlug pins and unpins without a max count', () => {
    expect(togglePinnedSlug('a', storage)).toEqual(['a']);
    expect(togglePinnedSlug('b', storage)).toEqual(['a', 'b']);
    expect(togglePinnedSlug('c', storage)).toEqual(['a', 'b', 'c']);
    expect(togglePinnedSlug('d', storage)).toEqual(['a', 'b', 'c', 'd']);
    expect(togglePinnedSlug('b', storage)).toEqual(['a', 'c', 'd']);
    expect(isPinnedSlug('a', readPinnedSlugs(storage))).toBe(true);
    expect(isPinnedSlug('b', readPinnedSlugs(storage))).toBe(false);
  });

  it('splitPinnedTeams shows all when no pins', () => {
    const teams = [
      { id: 1, slug: 't1' },
      { id: 2, slug: 't2' },
      { id: 3, slug: 't3' },
    ];
    const { pinned, other } = splitPinnedTeams(teams, []);
    expect(pinned.map((t) => t.slug)).toEqual(['t1', 't2', 't3']);
    expect(other).toEqual([]);
  });

  it('splitPinnedTeams keeps pin order and puts the rest in other', () => {
    const teams = [
      { id: 1, slug: 't1' },
      { id: 2, slug: 't2' },
      { id: 3, slug: 't3' },
      { id: 4, slug: 't4' },
    ];
    const { pinned, other } = splitPinnedTeams(teams, ['t4', 't2']);
    expect(pinned.map((t) => t.slug)).toEqual(['t4', 't2']);
    expect(other.map((t) => t.slug)).toEqual(['t1', 't3']);
  });
});
