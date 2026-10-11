import { describe, expect, it } from 'vitest';
import { resolveQuickCreateTeam } from './quickCreateTeam';

describe('resolveQuickCreateTeam', () => {
  it('チームWiki: 明示されたチームをそのまま使う', () => {
    expect(resolveQuickCreateTeam(7, [], null)).toEqual({ teamId: 7, needsChoice: false, choices: [] });
  });

  it('プロジェクトWiki: 参加チームが1つなら自動で決まる', () => {
    expect(resolveQuickCreateTeam(null, [{ id: 3 }], null).teamId).toBe(3);
  });

  it('プロジェクトWiki: アーカイブ済みチームは数えない', () => {
    const r = resolveQuickCreateTeam(null, [{ id: 3 }, { id: 4, archived: true }], null);
    expect(r.teamId).toBe(3);
    expect(r.needsChoice).toBe(false);
  });

  it('プロジェクトWiki: 複数チームなら選ぶまで決まらない', () => {
    const r = resolveQuickCreateTeam(null, [{ id: 3 }, { id: 4 }], null);
    expect(r.teamId).toBeNull();
    expect(r.needsChoice).toBe(true);
  });

  it('複数チームで選択済みなら、そのチームになる', () => {
    expect(resolveQuickCreateTeam(null, [{ id: 3 }, { id: 4 }], 4).teamId).toBe(4);
  });

  it('候補に無いチームが選択されていても使わない', () => {
    expect(resolveQuickCreateTeam(null, [{ id: 3 }, { id: 4 }], 9).teamId).toBeNull();
  });

  it('全体Wiki: チームもプロジェクトも無ければ決まらず、選択肢も出さない', () => {
    const r = resolveQuickCreateTeam(null, [], null);
    expect(r).toEqual({ teamId: null, needsChoice: false, choices: [] });
  });
});
