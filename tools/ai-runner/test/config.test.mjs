import test from 'node:test';
import assert from 'node:assert/strict';
import { parseConfig, parseActiveHours, isWithinActiveHours } from '../src/config.mjs';
import { baseEnv, cfg } from './helpers.mjs';

test('正しい設定は通り、安全モードが既定で有効', () => {
  const c = cfg();
  assert.equal(c.safeMode, true);
  assert.deepEqual(c.allowedRepos, ['acme/app']);
});
test('必須の設定が無いと拒否する', () => {
  for (const k of ['SENN_URL', 'RUNNER_TOKEN', 'RUNNER_SIGNING_SECRET', 'CLAUDE_CODE_OAUTH_TOKEN', 'ALLOWED_REPOS']) {
    const env = { ...baseEnv }; delete env[k];
    assert.throws(() => parseConfig(env), /設定エラー/, k);
  }
});
test('http は拒否。localhost だけ明示した場合に許可', () => {
  assert.throws(() => cfg({ SENN_URL: 'http://senn.example.com' }), /https/);
  assert.throws(() => cfg({ SENN_URL: 'http://localhost:3000' }), /https/);
  assert.doesNotThrow(() => cfg({ SENN_URL: 'http://localhost:3000', ALLOW_INSECURE_LOCALHOST: '1' }));
  assert.throws(() => cfg({ SENN_URL: 'http://evil.example.com', ALLOW_INSECURE_LOCALHOST: '1' }), /https/);
});
test('API キー(従量課金)が設定されていると起動を拒否する', () => {
  assert.throws(() => cfg({ ANTHROPIC_API_KEY: 'sk-ant-api03-x' }), /ANTHROPIC_API_KEY/);
  assert.throws(() => cfg({ ANTHROPIC_AUTH_TOKEN: 'x' }), /拒否/);
  assert.throws(() => cfg({ ANTHROPIC_BASE_URL: 'https://proxy' }), /拒否/);
});
test('リポジトリの許可リストの形式を検証する', () => {
  assert.throws(() => cfg({ ALLOWED_REPOS: 'acme/app,bad repo' }), /不正/);
  assert.throws(() => cfg({ ALLOWED_REPOS: '' }), /ALLOWED_REPOS/);
});
test('上限は整数のみ', () => {
  assert.throws(() => cfg({ CEILING_MAX_MINUTES: '0' }), /CEILING_MAX_MINUTES/);
  assert.throws(() => cfg({ CEILING_MAX_TURNS: 'abc' }), /CEILING_MAX_TURNS/);
  assert.equal(cfg({ CEILING_MAX_MINUTES: '60' }).ceilings.maxMinutes, 60);
});
test('実行時間帯: 通常と日またぎ', () => {
  const d = (h) => new Date(2026, 9, 7, h, 0, 0);
  const day = parseActiveHours('9-21');
  assert.equal(isWithinActiveHours(day, d(9)), true);
  assert.equal(isWithinActiveHours(day, d(20)), true);
  assert.equal(isWithinActiveHours(day, d(21)), false);
  assert.equal(isWithinActiveHours(day, d(3)), false);
  const night = parseActiveHours('22-6');
  assert.equal(isWithinActiveHours(night, d(23)), true);
  assert.equal(isWithinActiveHours(night, d(5)), true);
  assert.equal(isWithinActiveHours(night, d(12)), false);
  assert.equal(isWithinActiveHours(null, d(3)), true);
  assert.throws(() => parseActiveHours('abc'));
  assert.throws(() => parseActiveHours('5-5'));
});

test('Cloudflare Access のサービストークンは、2 つそろえて指定する', () => {
  assert.equal(cfg().cfAccess, null);
  assert.deepEqual(cfg({ CF_ACCESS_CLIENT_ID: 'a', CF_ACCESS_CLIENT_SECRET: 'b' }).cfAccess, { id: 'a', secret: 'b' });
  assert.throws(() => cfg({ CF_ACCESS_CLIENT_ID: 'a' }), /そろえて/);
  assert.throws(() => cfg({ CF_ACCESS_CLIENT_SECRET: 'b' }), /そろえて/);
});
