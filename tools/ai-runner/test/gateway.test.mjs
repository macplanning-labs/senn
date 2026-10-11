import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { createGateway, rejectReason, UPSTREAM } from '../gateway/gateway.mjs';
import { buildGatewayArgs } from '../src/executor.mjs';
import { cfg, makeJob } from './helpers.mjs';
import { verifyJob } from '../src/verify.mjs';

const REAL = 'sk-ant-oat01-real-token-for-test-0123456789';

/** 偽の Anthropic。受け取った要求を記録し、SSE っぽく 2 回に分けて返す */
async function fakeAnthropic() {
  const seen = [];
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (c) => (body += c));
    req.on('end', () => {
      seen.push({ method: req.method, url: req.url, headers: req.headers, body });
      res.writeHead(200, { 'content-type': 'text/event-stream', 'set-cookie': 'a=b' });
      res.write('event: message_start\ndata: {}\n\n');
      setTimeout(() => res.end('event: message_stop\ndata: {}\n\n'), 10);
    });
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  return { server, seen, url: `http://127.0.0.1:${server.address().port}` };
}

async function withGateway(fn) {
  const up = await fakeAnthropic();
  const logs = [];
  const gw = createGateway({ token: REAL, upstream: up.url, log: (l) => logs.push(l) });
  await new Promise((r) => gw.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${gw.address().port}`;
  try { await fn({ base, up, logs }); } finally { gw.close(); up.server.close(); }
}

const send = (base, path, { method = 'POST', headers = {}, body } = {}) =>
  fetch(base + path, { method, headers: { 'content-type': 'application/json', ...headers }, body: body === undefined ? undefined : JSON.stringify(body) });

test('宛先は api.anthropic.com に固定されている', () => {
  assert.equal(UPSTREAM, 'https://api.anthropic.com');
});

test('AI から来た認証情報を捨て、本物のトークンに差し替えて送る(応答はそのまま流す)', async () => {
  await withGateway(async ({ base, up, logs }) => {
    const r = await send(base, '/v1/messages?beta=true', {
      headers: { authorization: 'Bearer sk-ant-oat01-senn-gateway-placeholder', 'x-api-key': 'sk-ant-attacker', cookie: 'x=y', 'anthropic-beta': 'oauth-2025-04-20', 'anthropic-version': '2023-06-01' },
      body: { model: 'claude', max_tokens: 10, messages: [{ role: 'user', content: 'hi' }], tools: [{ name: 'Read', input_schema: { type: 'object' } }] },
    });
    assert.equal(r.status, 200);
    const text = await r.text();
    assert.match(text, /message_start[\s\S]*message_stop/);
    assert.equal(r.headers.get('set-cookie'), null);
    const got = up.seen.at(-1);
    assert.equal(got.url, '/v1/messages?beta=true');
    assert.equal(got.headers.authorization, `Bearer ${REAL}`);
    assert.equal(got.headers['x-api-key'], undefined, '攻撃者の API キーは捨てる');
    assert.equal(got.headers.cookie, undefined);
    assert.equal(got.headers['anthropic-beta'], 'oauth-2025-04-20');
    assert.ok(!logs.join('\n').includes(REAL), 'ログにトークンを出さない');
  });
});

test('許可していない宛先は断る(Anthropic には送らない)', async () => {
  await withGateway(async ({ base, up }) => {
    for (const [method, path] of [['POST', '/v1/files'], ['GET', '/v1/messages'], ['POST', '/v1/messages/batches'], ['GET', '/api/oauth/profile']]) {
      const r = await send(base, path, { method, body: method === 'POST' ? {} : undefined });
      assert.equal(r.status, 403, `${method} ${path}`);
    }
    // パスの「..」は、解決した形で判定し、その形で送る(許可したパスの下から別の宛先へは抜けられない)
    const raw = await new Promise((resolve) => {
      const u = new URL(base);
      const req = http.request({ host: u.hostname, port: u.port, method: 'POST', path: '/v1/messages/../files', headers: { 'content-type': 'application/json' } }, (res) => { res.resume(); resolve(res.statusCode); });
      req.end('{}');
    });
    assert.equal(raw, 403);
    assert.equal(up.seen.length, 0);
  });
});

test('Anthropic のサーバーで動く道具や MCP の指定を含む要求は断る(データを外へ送らせる抜け道)', async () => {
  await withGateway(async ({ base, up }) => {
    const bad = [
      { tools: [{ type: 'web_fetch_20250910', name: 'web_fetch' }] },
      { tools: [{ type: 'web_search_20250305', name: 'web_search' }] },
      { tools: [{ type: 'code_execution_20250825', name: 'code_execution' }] },
      { mcp_servers: [{ type: 'url', url: 'https://evil.example/mcp', name: 'x' }] },
      { container: 'container_abc' },
      { tools: 'not-an-array' },
    ];
    for (const b of bad) {
      const r = await send(base, '/v1/messages', { body: { model: 'm', max_tokens: 1, messages: [], ...b } });
      assert.equal(r.status, 403, JSON.stringify(b));
    }
    const r = await fetch(base + '/v1/messages', { method: 'POST', body: '{not json' });
    assert.equal(r.status, 400);
    assert.equal(up.seen.length, 0);
  });
});

test('手元で実行される道具(Claude Code の道具、bash・テキスト編集など)は通す', () => {
  assert.equal(rejectReason({ tools: [{ name: 'Bash', input_schema: {} }, { type: 'custom', name: 'X' }, { type: 'bash_20250124', name: 'bash' }, { type: 'text_editor_20250728', name: 'str_replace_based_edit_tool' }] }), null);
  assert.equal(rejectReason({ messages: [] }), null);
  assert.notEqual(rejectReason({ tools: [{ type: 'web_fetch_20250910' }] }), null);
  assert.notEqual(rejectReason([]), null);
});

test('トークンが無ければ起動しない', () => {
  assert.throws(() => createGateway({ token: '' }), /CLAUDE_CODE_OAUTH_TOKEN/);
});

test('中継役のコンテナ: 隔離され、トークンは値ではなく名前だけで渡す', () => {
  const c = cfg();
  const job = verifyJob(makeJob(c), c);
  const a = buildGatewayArgs({ config: c, job, gatewayDir: '/opt/gw', user: { uid: 501, gid: 20 } });
  for (const need of ['--read-only', '--cap-drop=ALL', '--rm']) assert.ok(a.includes(need), need);
  assert.ok(a.includes('CLAUDE_CODE_OAUTH_TOKEN'));
  assert.ok(!a.join(' ').includes(c.oauth));
  assert.equal(a[a.indexOf('--network') + 1], 'senn-ai-egress');
  assert.deepEqual(a.filter((x, i) => a[i - 1] === '-v'), ['/opt/gw:/opt/gateway:ro']);
  assert.ok(!a.join(' ').includes('docker.sock'));
});
