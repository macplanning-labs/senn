import { describe, it, expect, beforeAll, vi } from 'vitest';

type BuildNavItems = (
  teamSlug: string | null,
  projectKey: string | null,
) => Array<{ id: string; label: string; path: string; icon: string }>;

let buildNavItems: BuildNavItems;

beforeAll(async () => {
  // テスト環境は node のため、store 初期化が参照する localStorage を最小限スタブする
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
    clear: () => store.clear(),
  });
  ({ buildNavItems } = await import('./CommandPalette'));
});

describe('buildNavItems', () => {
  it('チームを開いているとき、チームのガント・依存関係・受信箱への入口が含まれる', () => {
    const items = buildNavItems('dev', null);
    expect(items.find((i) => i.id === 'team-gantt')?.path).toBe('/team/dev/gantt');
    expect(items.find((i) => i.id === 'team-dependencies')?.path).toBe('/team/dev/dependencies');
    expect(items.find((i) => i.id === 'team-triage')?.path).toBe('/team/dev/triage');
  });

  it('チーム未選択のときは、チームのガント・依存関係・受信箱は出ない', () => {
    const items = buildNavItems(null, null);
    expect(items.some((i) => i.id === 'team-gantt')).toBe(false);
    expect(items.some((i) => i.id === 'team-dependencies')).toBe(false);
    expect(items.some((i) => i.id === 'team-triage')).toBe(false);
  });

  it('グローバル Wiki 入口が常に含まれる', () => {
    const items = buildNavItems(null, null);
    expect(items.find((i) => i.id === 'workspace-wiki')?.path).toBe('/wiki');
  });

  it('プロジェクトを開いているとき、プロジェクトのガント・依存関係が含まれる', () => {
    const items = buildNavItems(null, 'ABC');
    expect(items.find((i) => i.id === 'gantt')?.path).toBe('/project/ABC/gantt');
    expect(items.find((i) => i.id === 'dependencies')?.path).toBe('/project/ABC/dependencies');
  });
});
