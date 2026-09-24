import { describe, it, expect } from 'vitest';
import i18n from '@/i18n';
import { projectStatusLabelKey } from './projectStatus';

describe('projectStatusLabelKey', () => {
  it('4つのステータスに、実在する翻訳キーを返す(in_progress は inProgress)', () => {
    for (const status of ['planned', 'in_progress', 'paused', 'completed']) {
      const key = projectStatusLabelKey(status);
      expect(key, status).not.toBeNull();
      for (const lng of ['ja', 'en']) {
        // キーが実在すること(存在しないキーは、キー名がそのまま返る)
        expect(i18n.t(key as string, { lng }), `${lng}:${key}`).not.toBe(key);
      }
    }
    expect(projectStatusLabelKey('in_progress')).toBe('sidebar.projectStatus.inProgress');
  });

  it('未知の値は null(画面は生の値を出す)', () => {
    expect(projectStatusLabelKey('active')).toBeNull();
    expect(projectStatusLabelKey('')).toBeNull();
  });
});
