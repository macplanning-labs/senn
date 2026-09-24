import { describe, it, expect } from 'vitest';
import { projectStatusLabelKey } from './teamArchiveCheck';

describe('projectStatusLabelKey', () => {
  it('planned ステータスに対応するキーを返す', () => {
    expect(projectStatusLabelKey('planned')).toBe('sidebar.projectStatus.planned');
  });

  it('in_progress ステータスに対応するキーを返す', () => {
    expect(projectStatusLabelKey('in_progress')).toBe('sidebar.projectStatus.inProgress');
  });

  it('paused ステータスに対応するキーを返す', () => {
    expect(projectStatusLabelKey('paused')).toBe('sidebar.projectStatus.paused');
  });

  it('completed ステータスに対応するキーを返す', () => {
    expect(projectStatusLabelKey('completed')).toBe('sidebar.projectStatus.completed');
  });

  it('未知のステータスに対しては null を返す', () => {
    expect(projectStatusLabelKey('unknown')).toBeNull();
  });
});
