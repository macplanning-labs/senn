import { describe, it, expect } from 'vitest';
import { integrationQueryKeys } from './integrationQueryKeys';

describe('integrationQueryKeys', () => {
  it('連携の一覧と、リポジトリ一覧(連携済みの印)の両方を含む', () => {
    expect(integrationQueryKeys('team:7')).toEqual([
      ['integrations', 'team:7'],
      ['github-app', 'repositories', 'team:7'],
    ]);
  });

  it('範囲(チーム/プロジェクト)ごとに別のキーになる', () => {
    expect(integrationQueryKeys('project:3')).not.toEqual(integrationQueryKeys('team:3'));
  });
});
