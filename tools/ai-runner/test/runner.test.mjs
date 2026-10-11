import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Runner } from '../src/runner.mjs';
import { cfg, makeJob } from './helpers.mjs';

function setup({ job, sandbox, finalize, startResp = { stop: false }, hbResp = { stop: false }, extraEnv = {} } = {}) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'runner-test-'));
  const config = cfg({ RUNNER_HOME: home, ...extraEnv });
  const calls = [];
  const client = {
    claim: async (accepting) => { calls.push(['claim', accepting]); return { job: job ? job(config) : null }; },
    start: async (u) => { calls.push(['start', u]); return startResp; },
    heartbeat: async (u, usage) => { calls.push(['heartbeat', usage]); return hbResp; },
    finish: async (u, b) => { calls.push(['finish', b]); return {}; },
    error: async (u, b) => { calls.push(['error', b]); return {}; },
  };
  const deps = {
    prepareWorkspace: async (c, j, dir) => ({ workspace: dir + '/ws', gitDir: dir + '/gitdir' }),
    runSandbox: sandbox ?? (async (c, j, ws, { meter }) => {
      meter.feed(JSON.stringify({ type: 'assistant', message: { id: 'm1', usage: { input_tokens: 100, output_tokens: 50 } } }));
      meter.feed(JSON.stringify({ type: 'result', result: '修正しました', is_error: false, total_cost_usd: 0.5 }));
      return { code: 0, killedBy: null, stderr: '' };
    }),
    finalizeWork: finalize ?? (async () => ({ changed: true, pushed: false, note: '安全モードのため push していません' })),
  };
  const runner = new Runner({ config, client, deps, log: () => {}, sleep: async () => {} });
  return { runner, calls, config, home };
}
const last = (calls, name) => calls.filter((c) => c[0] === name).at(-1)?.[1];

test('正常系: 着手 → 実行 → 完了(resolved)と使用量の報告', async () => {
  const { runner, calls } = setup({ job: (c) => makeJob(c) });
  assert.equal(await runner.tick(), true);
  assert.deepEqual(calls.map((c) => c[0]), ['claim', 'start', 'finish']);
  const f = last(calls, 'finish');
  assert.equal(f.result, 'resolved');
  assert.equal(f.usage.inputTokens, 100);
  assert.equal(f.usdEquivalent, 0.5);
  assert.match(f.summary, /修正しました/);
});
test('依頼が無ければ何もしない', async () => {
  const { runner, calls } = setup({});
  assert.equal(await runner.tick(), false);
  assert.deepEqual(calls.map((c) => c[0]), ['claim']);
});
test('署名が不正な依頼は、実行せずに失敗として報告する', async () => {
  let ran = false;
  const { runner, calls } = setup({ job: (c) => makeJob(c, { signature: 'f'.repeat(64) }), sandbox: async () => { ran = true; return {}; } });
  await runner.tick();
  assert.equal(ran, false);
  assert.ok(!calls.some((c) => c[0] === 'start'));
  assert.match(last(calls, 'error').detail, /署名/);
});
test('許可リスト外のリポジトリは実行しない', async () => {
  let ran = false;
  const { runner, calls } = setup({ job: (c) => makeJob(c, { repo: 'evil/app' }), sandbox: async () => { ran = true; return {}; } });
  await runner.tick();
  assert.equal(ran, false);
  assert.ok(calls.some((c) => c[0] === 'error'));
});
test('開始時に停止を指示されたら、実行せず aborted で終える', async () => {
  let ran = false;
  const { runner, calls } = setup({ job: (c) => makeJob(c), startResp: { stop: true, reason: 'killed' }, sandbox: async () => { ran = true; return {}; } });
  await runner.tick();
  assert.equal(ran, false);
  assert.equal(last(calls, 'finish').result, 'aborted');
});
test('サンドボックスが停止された場合は aborted(push・コミットはしない)', async () => {
  let finalized = false;
  const { runner, calls } = setup({
    job: (c) => makeJob(c),
    sandbox: async () => ({ code: 137, killedBy: 'per_run_tokens', stderr: '' }),
    finalize: async () => { finalized = true; return {}; },
  });
  await runner.tick();
  assert.equal(finalized, false);
  const f = last(calls, 'finish');
  assert.equal(f.result, 'aborted');
  assert.match(f.summary, /per_run_tokens/);
});
test('ハートビートで停止を指示されると、shouldStop がその理由を返す', async () => {
  let reason = null;
  const { runner, calls } = setup({
    job: (c) => makeJob(c),
    extraEnv: { HEARTBEAT_SECONDS: '5' },
    hbResp: { stop: true, reason: 'burn_rate' },
    sandbox: async (c, j, ws, { shouldStop }) => {
      await new Promise((r) => setTimeout(r, 5200));
      reason = shouldStop();
      return { code: 137, killedBy: reason, stderr: '' };
    },
  });
  await runner.tick();
  assert.equal(reason, 'burn_rate');
  assert.ok(calls.some((c) => c[0] === 'heartbeat'));
  assert.match(last(calls, 'finish').summary, /burn_rate/);
});
test('claude が失敗(終了コード≠0)なら error で報告し、トークンは伏せる', async () => {
  const { runner, calls } = setup({
    job: (c) => makeJob(c),
    sandbox: async () => ({ code: 1, killedBy: null, stderr: 'boom ' + ['sk', 'ant', 'oat01'].join('-') + '-abcdefghijklmnopqrstuv' }),
  });
  await runner.tick();
  const e = last(calls, 'error');
  assert.ok(e.detail.includes('boom'));
  assert.ok(!e.detail.includes('sk-ant-oat01'));
  assert.ok(!calls.some((c) => c[0] === 'finish'));
});
test('ターン数の上限に達した場合は、その旨を error で報告する', async () => {
  const { runner, calls } = setup({
    job: (c) => makeJob(c),
    sandbox: async (c, j, ws, { meter }) => { meter.feed(JSON.stringify({ type: 'result', subtype: 'error_max_turns', is_error: true })); return { code: 1, killedBy: null, stderr: '' }; },
  });
  await runner.tick();
  assert.match(last(calls, 'error').detail, /ターン数/);
});
test('変更なし・点検でブロックされた場合は blocked', async () => {
  for (const [fin, re] of [[{ changed: false }, /変更がありません/], [{ changed: true, blocked: '秘密情報の疑いがあるため、保存しません' }, /秘密情報/]]) {
    const { runner, calls } = setup({ job: (c) => makeJob(c), finalize: async () => fin });
    await runner.tick();
    const f = last(calls, 'finish');
    assert.equal(f.result, 'blocked');
    assert.match(f.summary, re);
  }
});
test('準備(clone)の失敗は error で報告し、実行に進まない', async () => {
  let ran = false;
  const { runner, calls } = setup({ job: (c) => makeJob(c), sandbox: async () => { ran = true; return {}; } });
  runner.deps.prepareWorkspace = async () => { throw new Error('clone に失敗'); };
  await runner.tick();
  assert.equal(ran, false);
  assert.match(last(calls, 'error').detail, /clone/);
});
test('STOP ファイルがあると、依頼を受け付けない(接続だけ知らせる)', async () => {
  const { runner, calls, config } = setup({});
  fs.writeFileSync(config.stopFile, '');
  await runner.tick();
  assert.deepEqual(calls[0], ['claim', false]);
});
test('終了処理中は受け付けない', async () => {
  const { runner, calls } = setup({});
  runner.shutdown();
  await runner.tick();
  assert.deepEqual(calls[0], ['claim', false]);
});

// 2026-10-11 の点検: チケットに仕込まれた指示で、AI が最終報告に Claude のトークンを書くと、
// 報告はチケットのコメントになり、Guest も読めた。最終報告にもランナーの秘密の伏せ字をかける
test('最終報告に、Claude のトークン(そのまま・base64)が含まれていたら伏せる', async () => {
  let token;
  const { runner, calls } = setup({
    job: (c) => { token = c.oauth; return makeJob(c); },
    sandbox: async (c, j, ws, { meter }) => {
      const leak = `完了。${c.oauth} / ${Buffer.from(c.oauth).toString('base64')}`;
      meter.feed(JSON.stringify({ type: 'result', result: leak, is_error: false, total_cost_usd: 0 }));
      return { code: 0, killedBy: null, stderr: '' };
    },
  });
  await runner.tick();
  const summary = last(calls, 'finish').summary;
  assert.ok(token && token.length >= 8, 'テスト用の設定にトークンがある');
  assert.ok(!summary.includes(token), summary);
  assert.ok(!summary.includes(Buffer.from(token).toString('base64').slice(4, -4)), summary);
  assert.match(summary, /伏せ字/);
});
