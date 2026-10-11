import test from 'node:test';
import assert from 'node:assert/strict';
import { buildDockerArgs, assertSafeArgs, ALLOWED_TOOLS, DISALLOWED_TOOLS, PLACEHOLDER_TOKEN, gatewayName } from '../src/sandbox.mjs';
import { cfg, makeJob } from './helpers.mjs';
import { verifyJob } from '../src/verify.mjs';

function build(extra = {}, ws = '/tmp/work/ws') {
  const c = cfg(extra);
  const job = verifyJob(makeJob(c), c);
  return buildDockerArgs({ config: c, job, workspace: ws, hooksDir: '/opt/hooks', user: { uid: 501, gid: 20 } });
}

test('隔離の引数がそろっている', () => {
  const a = build();
  for (const need of ['--read-only', '--cap-drop=ALL', '--rm']) assert.ok(a.includes(need), need);
  assert.ok(a.join(' ').includes('--security-opt no-new-privileges'));
  assert.ok(a.join(' ').includes('--user 501:20'));
  assert.doesNotThrow(() => assertSafeArgs(a));
});
test('マウントは作業ディレクトリ(rw)と hooks(ro)の 2 つだけ', () => {
  const a = build();
  const mounts = a.filter((x, i) => a[i - 1] === '-v');
  assert.deepEqual(mounts, ['/tmp/work/ws:/workspace:rw', '/opt/hooks:/opt/ai-runner/hooks:ro']);
});
test('AI のコンテナには、本物のトークンを渡さない(偽のトークンと、中継役の宛先だけ)', () => {
  const c = cfg();
  const job = verifyJob(makeJob(c), c);
  const a = buildDockerArgs({ config: c, job, workspace: '/tmp/work/ws', hooksDir: '/opt/hooks', user: { uid: 501, gid: 20 } });
  assert.ok(a.includes(`CLAUDE_CODE_OAUTH_TOKEN=${PLACEHOLDER_TOKEN}`));
  assert.ok(!a.includes('CLAUDE_CODE_OAUTH_TOKEN'), '環境から引き継ぐ指定(名前だけ)も渡さない');
  assert.ok(!a.join(' ').includes(c.oauth), '本物のトークンは載らない');
  assert.ok(a.includes(`ANTHROPIC_BASE_URL=http://${gatewayName(job.runUuid)}:8080`));
});
test('本物のトークン・API キー・外へ出るプロキシを渡そうとすると拒否する', () => {
  const base = build();
  for (const bad of ['CLAUDE_CODE_OAUTH_TOKEN', 'CLAUDE_CODE_OAUTH_TOKEN=sk-ant-oat01-real', 'ANTHROPIC_API_KEY=sk-ant-x', 'ANTHROPIC_AUTH_TOKEN=x',
    'ANTHROPIC_BASE_URL=https://evil.example', 'HTTPS_PROXY=http://proxy:3128']) {
    assert.throws(() => assertSafeArgs([...base.slice(0, 2), '-e', bad, ...base.slice(2)]), Error, bad);
  }
});
test('プロンプトは引数に載らない(標準入力で渡す)', () => {
  assert.ok(!build().join(' ').includes('チケットに対応してください'));
});
test('--dangerously-skip-permissions は使わず、push や通信系は拒否リストにある', () => {
  const a = build().join(' ');
  assert.ok(!a.includes('dangerously'));
  assert.ok(a.includes('--permission-mode acceptEdits'));
  for (const t of ['Bash(git push:*)', 'WebFetch', 'Bash(curl:*)']) assert.ok(DISALLOWED_TOOLS.includes(t), t);
  assert.ok(ALLOWED_TOOLS.includes('Read'));
});
test('上限(ターン数)が claude に渡る', () => {
  const a = build();
  assert.equal(a[a.indexOf('--max-turns') + 1], '100');
});
test('プロキシを設定しても、AI のコンテナには渡さない(外へは中継役だけが出る)', () => {
  const a = build({ SANDBOX_PROXY_URL: 'http://proxy:3128' }).join(' ');
  assert.ok(!/HTTPS?_PROXY=/i.test(a));
  assert.doesNotThrow(() => assertSafeArgs(build({ SANDBOX_PROXY_URL: 'http://proxy:3128' })));
});
test('危険なパスと、禁止された引数を拒否する', () => {
  assert.throws(() => build({}, 'relative/path'));
  assert.throws(() => build({}, '/tmp/a b'));
  assert.throws(() => build({}, '/tmp/a:ro,/etc'));
  for (const bad of ['--privileged', '/var/run/docker.sock:/x', '--network=host', '--cap-add=SYS_ADMIN', '--dangerously-skip-permissions']) {
    assert.throws(() => assertSafeArgs([...build(), bad]), /禁止/, bad);
  }
  const noRo = build().filter((x) => x !== '--read-only');
  assert.throws(() => assertSafeArgs(noRo), /必須/);
});

test('サンドボックスは、Anthropic の API 以外への通信をしない設定で動かす', async () => {
  const { buildDockerArgs } = await import('../src/sandbox.mjs');
  const args = buildDockerArgs({
    config: { pids: 64, memory: '1g', cpus: '1', sandboxNetwork: 'senn-ai-internal', proxyUrl: 'http://proxy:3128', image: 'img' },
    job: { runUuid: '12345678-aaaa', caps: { maxTurns: 5 } },
    workspace: '/tmp/ws', hooksDir: '/tmp/hooks', user: { uid: 1000, gid: 1000 },
  });
  assert.ok(args.includes('CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1'));
});
