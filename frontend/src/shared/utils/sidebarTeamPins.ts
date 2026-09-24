/**
 * サイドバー常時表示チームの PIN（明示固定）。件数上限なし。
 * localStorage キー: senn-sidebar-team-pins
 */

export const SIDEBAR_TEAM_PINS_KEY = 'senn-sidebar-team-pins';

export type TeamLike = { id: number; slug: string };

export function readPinnedSlugs(storage: Storage = localStorage): string[] {
  try {
    const raw = storage.getItem(SIDEBAR_TEAM_PINS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((s): s is string => typeof s === 'string' && s.length > 0);
  } catch {
    return [];
  }
}

function writePinnedSlugs(slugs: string[], storage: Storage): string[] {
  storage.setItem(SIDEBAR_TEAM_PINS_KEY, JSON.stringify(slugs));
  return slugs;
}

/** PIN をトグル。返り値は新しい PIN 一覧（順序維持。新規は末尾追加） */
export function togglePinnedSlug(slug: string, storage: Storage = localStorage): string[] {
  const current = readPinnedSlugs(storage);
  if (current.includes(slug)) {
    return writePinnedSlugs(
      current.filter((s) => s !== slug),
      storage,
    );
  }
  return writePinnedSlugs([...current, slug], storage);
}

export function isPinnedSlug(slug: string, pinnedSlugs: string[]): boolean {
  return pinnedSlugs.includes(slug);
}

/**
 * PIN 済みチームを常時表示、未 PIN を「すべて表示」へ。
 * PIN が 1 件も無いときは、初回 UX のため全チームを常時表示（すべて表示は空）。
 */
export function splitPinnedTeams<T extends TeamLike>(
  activeTeams: T[],
  pinnedSlugs: string[],
): { pinned: T[]; other: T[] } {
  if (activeTeams.length === 0) {
    return { pinned: [], other: [] };
  }

  if (pinnedSlugs.length === 0) {
    return { pinned: [...activeTeams], other: [] };
  }

  const bySlug = new Map(activeTeams.map((t) => [t.slug, t]));
  const pinned: T[] = [];
  const used = new Set<string>();

  for (const slug of pinnedSlugs) {
    const team = bySlug.get(slug);
    if (team && !used.has(slug)) {
      pinned.push(team);
      used.add(slug);
    }
  }

  const other = activeTeams.filter((t) => !used.has(t.slug));
  return { pinned, other };
}
