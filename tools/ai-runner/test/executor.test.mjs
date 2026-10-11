import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { finalizeWork, runSandbox } from '../src/executor.mjs';
import { cfg, makeJob } from './helpers.mjs';
import { verifyJob } from '../src/verify.mjs';

function repo(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'exec-test-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const ws = path.join(dir, 'ws'); const gitDir = path.join(dir, 'gitdir');
  fs.mkdirSync(ws);
  const g = (...a) => execFileSync('git', [`--git-dir=${gitDir}`, `--work-tree=${ws}`, '-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { encoding: 'utf8' });
  execFileSync('git', ['init', '-q', '--separate-git-dir', gitDir, ws]);
  fs.rmSync(path.join(ws, '.git'), { force: true });
  fs.writeFileSync(path.join(ws, 'a.txt'), 'hello\n');
  g('add', '-A'); g('commit', '-q', '-m', 'init'); g('checkout', '-q', '-b', 'ai/senn-12');
  return { ws, gitDir, g, home: dir };
}
const setup = (t) => {
  const r = repo(t);
  const config = cfg({ RUNNER_HOME: path.join(r.home, 'home') });
  const job = verifyJob(makeJob(config), config);
  return { ...r, config, job };
};

test('変更があればコミットする(安全モードでは push しない)', async (t) => {
  const { ws, gitDir, g, config, job } = setup(t);
  fs.writeFileSync(path.join(ws, 'b.txt'), '追加\n');
  const r = await finalizeWork(config, job, { workspace: ws, gitDir }, () => {});
  assert.equal(r.changed, true); assert.equal(r.pushed, false); assert.match(r.note, /安全モード/);
  assert.match(g('log', '-1', '--format=%s'), /SENN-12/);
});
test('変更が無ければ何もしない', async (t) => {
  const { ws, gitDir, config, job } = setup(t);
  const r = await finalizeWork(config, job, { workspace: ws, gitDir }, () => {});
  assert.equal(r.changed, false);
});
test('秘密情報を含む変更は、コミットせずブロックする', async (t) => {
  const { ws, gitDir, g, config, job } = setup(t);
  fs.writeFileSync(path.join(ws, 'c.ts'), `const k = "${['sk', 'ant', 'api03'].join('-')}-abcdefghijklmnop1234";\n`);
  const r = await finalizeWork(config, job, { workspace: ws, gitDir }, () => {});
  assert.match(r.blocked, /秘密情報/);
  assert.equal(g('log', '--format=%s').trim(), 'init');
});
test('.env などのファイルはブロックする', async (t) => {
  const { ws, gitDir, config, job } = setup(t);
  fs.writeFileSync(path.join(ws, '.env'), 'X=1\n');
  assert.match((await finalizeWork(config, job, { workspace: ws, gitDir }, () => {})).blocked, /秘密情報/);
});
test('CI・フックなど保護された場所の変更はブロックする', async (t) => {
  const { ws, gitDir, config, job } = setup(t);
  fs.mkdirSync(path.join(ws, '.github/workflows'), { recursive: true });
  fs.writeFileSync(path.join(ws, '.github/workflows/x.yml'), 'on: push\n');
  assert.match((await finalizeWork(config, job, { workspace: ws, gitDir }, () => {})).blocked, /保護された場所/);
});
test('作業ディレクトリ内の .git や フックを AI が書き換えても、ホストの git は影響を受けない', async (t) => {
  const { ws, gitDir, g, config, job, home } = setup(t);
  const marker = path.join(home, 'pwned');
  // AI が仕掛けうるもの: 偽の .git ファイル・.git/hooks
  fs.writeFileSync(path.join(ws, '.git'), `gitdir: ${path.join(home, 'evil')}\n`);
  fs.mkdirSync(path.join(home, 'evil/hooks'), { recursive: true });
  fs.writeFileSync(path.join(home, 'evil/hooks/pre-commit'), `#!/bin/sh\ntouch ${marker}\n`, { mode: 0o755 });
  fs.writeFileSync(path.join(ws, 'd.txt'), 'x\n');
  const r = await finalizeWork(config, job, { workspace: ws, gitDir }, () => {});
  assert.equal(r.changed, true);
  assert.equal(fs.existsSync(marker), false);
  assert.match(g('log', '-1', '--format=%s'), /SENN-12/);
});

function fakeSpawn({ onKill }) {
  const procs = [];
  const spawnImpl = (cmd, args, opts) => {
    const p = new EventEmitter();
    p.stdin = new PassThrough(); p.stdout = new PassThrough(); p.stderr = new PassThrough();
    p.kill = () => {}; p.args = args; p.cmd = cmd; p.opts = opts; p.stdinData = '';
    p.stdin.on('data', (d) => (p.stdinData += d));
    if (args[0] === 'kill') onKill?.(p);
    procs.push(p);
    return p;
  };
  return { spawnImpl, procs };
}

const tick = () => new Promise((r) => setImmediate(r));
function fakeGw(log = []) {
  return { start: async (c, j) => { log.push(['start', j.runUuid]); }, stop: async (c, j) => { log.push(['stop', j.runUuid]); }, log };
}

test('runSandbox: プロンプトを標準入力で渡し、出力から使用量を数え、終了で結果を返す', async () => {
  const config = cfg(); const job = verifyJob(makeJob(config), config);
  const { spawnImpl, procs } = fakeSpawn({});
  const p = runSandbox(config, job, '/tmp/ws', { shouldStop: () => null, spawnImpl, tickMs: 20, gatewayImpl: fakeGw() });
  await tick();
  const main = procs[0];
  assert.equal(main.args[0], 'run');
  main.stdout.write(JSON.stringify({ type: 'assistant', message: { id: 'a', usage: { input_tokens: 7, output_tokens: 3 } } }) + '\n');
  main.stdout.write(JSON.stringify({ type: 'result', result: 'ok' }) + '\n');
  main.emit('close', 0);
  const r = await p;
  assert.equal(r.code, 0);
  assert.equal(main.stdinData, job.prompt);
  assert.equal(r.meter.weighted(), 10);
});
test('runSandbox: 停止の指示で docker kill を実行する', async () => {
  const config = cfg(); const job = verifyJob(makeJob(config), config);
  let killed = null;
  const { spawnImpl, procs } = fakeSpawn({ onKill: (p) => (killed = p.args) });
  const p = runSandbox(config, job, '/tmp/ws', { shouldStop: () => 'killed', spawnImpl, tickMs: 20, gatewayImpl: fakeGw() });
  await new Promise((r) => setTimeout(r, 80));
  assert.deepEqual(killed, ['kill', 'senn-ai-123e4567']);
  procs[0].emit('close', 137);
  assert.equal((await p).killedBy, 'killed');
});
test('runSandbox: トークン量が上限を超えたら止める', async () => {
  const config = cfg(); const job = { ...verifyJob(makeJob(config), config), caps: { maxTurns: 100, maxMinutes: 30, maxWeightedTokens: 1000 } };
  let killed = false;
  const { spawnImpl, procs } = fakeSpawn({ onKill: () => (killed = true) });
  const p = runSandbox(config, job, '/tmp/ws', { shouldStop: () => null, spawnImpl, tickMs: 20, gatewayImpl: fakeGw() });
  await tick();
  procs[0].stdout.write(JSON.stringify({ type: 'assistant', message: { id: 'a', usage: { input_tokens: 5000, output_tokens: 0 } } }) + '\n');
  await new Promise((r) => setTimeout(r, 80));
  assert.equal(killed, true);
  procs[0].emit('close', 137);
  assert.equal((await p).killedBy, 'per_run_tokens');
});
test('runSandbox: 時間の上限を超えたら止める', async () => {
  const config = cfg(); const job = { ...verifyJob(makeJob(config), config), caps: { maxTurns: 100, maxMinutes: 0.0005, maxWeightedTokens: 1e9 } };
  const { spawnImpl, procs } = fakeSpawn({});
  const p = runSandbox(config, job, '/tmp/ws', { shouldStop: () => null, spawnImpl, tickMs: 20, gatewayImpl: fakeGw() });
  await new Promise((r) => setTimeout(r, 150));
  procs[0].emit('close', 137);
  assert.equal((await p).killedBy, 'per_run_time');
});

test('runSandbox: 実行の前に中継役を立て、終わったら(失敗しても)必ず止める', async () => {
  const config = cfg(); const job = verifyJob(makeJob(config), config);
  const gw = fakeGw();
  const { spawnImpl, procs } = fakeSpawn({});
  const p = runSandbox(config, job, '/tmp/ws', { shouldStop: () => null, spawnImpl, tickMs: 20, gatewayImpl: gw });
  await tick();
  assert.deepEqual(gw.log, [['start', job.runUuid]]);
  procs[0].emit('close', 1);
  await p;
  assert.deepEqual(gw.log.map((x) => x[0]), ['start', 'stop']);
  // AI のコンテナを起動するときの環境に、本物のトークンを入れない
  assert.ok(procs[0].opts?.env, '環境が記録されている');
  assert.ok(!('CLAUDE_CODE_OAUTH_TOKEN' in procs[0].opts.env));
});
test('runSandbox: 中継役が起動できなければ、AI を動かさずに失敗を返す', async () => {
  const config = cfg(); const job = verifyJob(makeJob(config), config);
  const { spawnImpl, procs } = fakeSpawn({});
  const r = await runSandbox(config, job, '/tmp/ws', { shouldStop: () => null, spawnImpl, tickMs: 20,
    gatewayImpl: { start: async () => { throw new Error('no network'); }, stop: async () => {} } });
  assert.equal(r.code, -1);
  assert.match(r.stderr, /中継役/);
  assert.equal(procs.length, 0);
});
