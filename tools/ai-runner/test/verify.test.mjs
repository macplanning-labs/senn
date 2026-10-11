import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { verifyJob, signingString, sign, workBranch } from '../src/verify.mjs';
import { cfg, makeJob } from './helpers.mjs';

test('HMAC-SHA256 は標準の値と一致する(サーバーの実装と同じ式)', () => {
  const expected = crypto.createHmac('sha256', 'key').update('The quick brown fox jumps over the lazy dog').digest('hex');
  assert.equal(sign('key', 'The quick brown fox jumps over the lazy dog'), 'f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8');
  assert.equal(expected, 'f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8');
});
test('署名対象の文字列は、サーバーと同じ並び', () => {
  const c = cfg();
  const j = makeJob(c);
  assert.equal(signingString(j), `${j.runUuid}\nSENN-12\n${j.promptHash}\nacme/app\ndevelop\n100\n30\n5000000`);
});
test('正しい依頼は通り、上限は自分の絶対上限との小さいほうになる', () => {
  const c = cfg({ CEILING_MAX_MINUTES: '10', CEILING_MAX_TURNS: '500' });
  const v = verifyJob(makeJob(c), c);
  assert.equal(v.caps.maxMinutes, 10);
  assert.equal(v.caps.maxTurns, 100);
  assert.equal(v.caps.maxWeightedTokens, 5_000_000);
});
test('SENN が大きな上限を送っても、絶対上限を超えない', () => {
  const c = cfg();
  const v = verifyJob(makeJob(c, { caps: { maxTurns: 99999, maxMinutes: 99999, maxWeightedTokens: 9e12 } }), c);
  assert.deepEqual(v.caps, { maxTurns: 400, maxMinutes: 180, maxWeightedTokens: 30_000_000 });
});
test('改ざんを拒否する(署名・プロンプト・リポジトリ・上限)', () => {
  const c = cfg();
  const good = makeJob(c);
  assert.throws(() => verifyJob({ ...good, signature: 'x'.repeat(64) }, c), /署名/);
  assert.throws(() => verifyJob({ ...good, prompt: good.prompt + '追記' }, c), /ハッシュ/);
  assert.throws(() => verifyJob({ ...good, repo: 'evil/app' }, c), /署名/);
  assert.throws(() => verifyJob({ ...good, caps: { ...good.caps, maxMinutes: 31 } }, c), /署名/);
  const other = cfg({ RUNNER_SIGNING_SECRET: 'another-secret' });
  assert.throws(() => verifyJob(good, other), /署名/);
});
test('署名が正しくても、許可リスト外のリポジトリは拒否する', () => {
  const c = cfg({ ALLOWED_REPOS: 'acme/other' });
  assert.throws(() => verifyJob(makeJob(c, { repo: 'acme/app' }), c), /許可されていない/);
});
test('形式の不正を拒否する', () => {
  const c = cfg();
  for (const over of [
    { runUuid: 'not-a-uuid' }, { ticketKey: 'senn-1; rm -rf /' }, { repo: 'a/b/c' },
    { baseBranch: '../../etc' }, { baseBranch: '-x' }, { baseBranch: 'a..b' }, { baseBranch: 'x.lock' },
    { caps: { maxTurns: 0, maxMinutes: 1, maxWeightedTokens: 1 } },
  ]) {
    assert.throws(() => verifyJob(makeJob(c, over), c), Error, JSON.stringify(over));
  }
  assert.throws(() => verifyJob(null, c));
});
test('作業ブランチ名は固定形式', () => {
  assert.equal(workBranch({ ticketKey: 'SENN-12' }), 'ai/senn-12');
});
