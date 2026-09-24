import { describe, it, expect } from 'vitest';
import { projectsForTeam } from './projectChoice';

const projects = [
  { id: 1, teams: [{ id: 10 }, { id: 20 }] },
  { id: 2, teams: [{ id: 20 }] },
  { id: 3, teams: [] },
  { id: 4 },
];

describe('projectsForTeam', () => {
  it('チームが参加しているプロジェクトだけを返す', () => {
    expect(projectsForTeam(projects, 20).map((p) => p.id)).toEqual([1, 2]);
    expect(projectsForTeam(projects, 10).map((p) => p.id)).toEqual([1]);
  });

  it('どのプロジェクトにも参加していないチームは空', () => {
    expect(projectsForTeam(projects, 99)).toEqual([]);
  });

  it('チーム未選択なら全プロジェクトを返す', () => {
    expect(projectsForTeam(projects, null)).toHaveLength(4);
  });
});
