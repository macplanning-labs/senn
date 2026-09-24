import { describe, it, expect } from 'vitest';
import { buildTeamUpdateData } from './teamUpdatePayload';

const base = {
  name: ' 開発チーム ',
  description: ' 説明 ',
  icon: '👥',
  color: '#6366f1',
  slackWebhookUrl: '',
  prefix: ' dev ',
  isActive: true,
};

describe('buildTeamUpdateData', () => {
  it('現在の「有効」を、そのまま送る(未指定だとバックエンドが false にしてしまう)', () => {
    expect(buildTeamUpdateData({ ...base, isActive: true }).isActive).toBe(true);
    expect(buildTeamUpdateData({ ...base, isActive: false }).isActive).toBe(false);
  });

  it('名前・説明・Prefix を整える(前後の空白を除き、Prefix は大文字)', () => {
    const d = buildTeamUpdateData(base);
    expect(d.name).toBe('開発チーム');
    expect(d.description).toBe('説明');
    expect(d.prefix).toBe('DEV');
  });

  it('Prefix が空なら送らず、Webhook が空なら undefined にする', () => {
    const d = buildTeamUpdateData({ ...base, prefix: '   ', slackWebhookUrl: '   ' });
    expect('prefix' in d).toBe(false);
    expect(d.slackWebhookUrl).toBeUndefined();
  });
});
