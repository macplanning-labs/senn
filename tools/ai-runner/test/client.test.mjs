import test from 'node:test';
import assert from 'node:assert/strict';
import { SennClient, withRetry } from '../src/client.mjs';

const resp = (status, body) => ({ ok: status < 400, status, text: async () => JSON.stringify(body) });

test('トークンをヘッダーで送り、パスが正しい', async () => {
  let seen;
  const c = new SennClient({ baseUrl: 'https://s', token: 'srun_x', fetchImpl: async (u, o) => { seen = { u, o }; return resp(200, { job: null }); } });
  await c.claim(true);
  assert.equal(seen.u, 'https://s/api/v1/ai-agent/runner/claim/');
  assert.equal(seen.o.headers['X-Runner-Token'], 'srun_x');
  assert.deepEqual(JSON.parse(seen.o.body), { accepting: true });
});
test('エラー応答は status つきの Error', async () => {
  const c = new SennClient({ baseUrl: 'https://s', token: 't', fetchImpl: async () => resp(401, { detail: '認証に失敗' }) });
  await assert.rejects(() => c.claim(), (e) => e.status === 401 && /認証に失敗/.test(e.message));
});
test('要約は長さを切り詰めて送る', async () => {
  let body;
  const c = new SennClient({ baseUrl: 'https://s', token: 't', fetchImpl: async (u, o) => { body = JSON.parse(o.body); return resp(200, {}); } });
  await c.finish('u', { result: 'resolved', summary: 'あ'.repeat(5000), usage: { inputTokens: 1 } });
  assert.equal(body.summary.length, 1900);
  assert.equal(body.inputTokens, 1);
});
test('withRetry: 5xx は再試行、4xx は即失敗', async () => {
  let n = 0;
  const sleep = async () => {};
  const r = await withRetry(async () => { if (++n < 3) { const e = new Error('x'); e.status = 503; throw e; } return 'ok'; }, { sleep });
  assert.equal(r, 'ok'); assert.equal(n, 3);
  let m = 0;
  await assert.rejects(() => withRetry(async () => { m++; const e = new Error('x'); e.status = 400; throw e; }, { sleep }));
  assert.equal(m, 1);
  let k = 0;
  await assert.rejects(() => withRetry(async () => { k++; throw new Error('net'); }, { sleep, tries: 3 }));
  assert.equal(k, 3);
});

test('Cloudflare Access のサービストークンを、設定したときだけヘッダーに付ける', async () => {
  let h1, h2;
  const mk = (cfAccess, set) => new SennClient({ baseUrl: 'https://s', token: 't', cfAccess, fetchImpl: async (u, o) => { set(o.headers); return resp(200, {}); } });
  await mk({ id: 'id1', secret: 'sec1' }, (h) => (h1 = h)).claim();
  await mk(null, (h) => (h2 = h)).claim();
  assert.equal(h1['CF-Access-Client-Id'], 'id1');
  assert.equal(h1['CF-Access-Client-Secret'], 'sec1');
  assert.ok(!('CF-Access-Client-Id' in h2));
});
test('Cloudflare Access に止められたときは、原因が分かるエラーにする', async () => {
  const blocked = { ok: false, status: 403, headers: { get: (k) => (k === 'cf-access-domain' ? 'x' : null) }, text: async () => '<html>' };
  const c = new SennClient({ baseUrl: 'https://s', token: 't', fetchImpl: async () => blocked });
  await assert.rejects(() => c.claim(), (e) => e.status === 403 && /Cloudflare Access/.test(e.message));
});
