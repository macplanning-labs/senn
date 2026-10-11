// client.mjs — SENN のランナー用 API の呼び出し。
const BASE = '/api/v1/ai-agent/runner';

export class SennClient {
  constructor({ baseUrl, token, cfAccess = null, fetchImpl = fetch, timeoutMs = 30_000 }) {
    this.baseUrl = baseUrl;
    this.token = token;
    this.cfAccess = cfAccess;
    this.fetch = fetchImpl;
    this.timeoutMs = timeoutMs;
  }

  async #post(path, body) {
    const res = await this.fetch(`${this.baseUrl}${BASE}${path}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Runner-Token': this.token,
        ...(this.cfAccess ? { 'CF-Access-Client-Id': this.cfAccess.id, 'CF-Access-Client-Secret': this.cfAccess.secret } : {}),
      },
      body: JSON.stringify(body ?? {}),
      signal: AbortSignal.timeout(this.timeoutMs),
    });
    const text = await res.text();
    let json = null;
    try { json = text ? JSON.parse(text) : null; } catch { /* 本文が JSON でない */ }
    if (!res.ok) {
      // Cloudflare Access に止められたとき(ログイン画面への誘導・403 の HTML)は、原因が分かる言葉にする
      if (res.headers?.get?.('cf-access-domain') || /cloudflareaccess|cf-access/i.test(text.slice(0, 2000))) {
        const err = new Error(`Cloudflare Access に止められました(${res.status})。CF_ACCESS_CLIENT_ID / CF_ACCESS_CLIENT_SECRET を設定してください`);
        err.status = res.status;
        throw err;
      }
      const err = new Error(`SENN API ${path} が ${res.status} を返しました: ${json?.detail ?? text.slice(0, 200)}`);
      err.status = res.status;
      throw err;
    }
    return json;
  }

  claim(accepting = true) { return this.#post('/claim/', { accepting }); }
  start(runUuid, model) { return this.#post(`/runs/${runUuid}/start/`, model ? { model } : {}); }
  heartbeat(runUuid, usage) { return this.#post(`/runs/${runUuid}/heartbeat/`, usage); }
  finish(runUuid, { result, summary, usage, usdEquivalent }) {
    return this.#post(`/runs/${runUuid}/finish/`, { result, summary: summary.slice(0, 1900), ...usage, ...(usdEquivalent !== undefined ? { usdEquivalent } : {}) });
  }
  error(runUuid, { detail, usage }) {
    return this.#post(`/runs/${runUuid}/error/`, { detail: detail.slice(0, 1900) || '原因不明', ...usage });
  }
}

/** 一時的な通信エラーだけ、数回やり直す(4xx は、やり直さない)。 */
export async function withRetry(fn, { tries = 4, baseMs = 500, sleep = (ms) => new Promise((r) => setTimeout(r, ms)) } = {}) {
  let last;
  for (let i = 0; i < tries; i++) {
    try { return await fn(); } catch (e) {
      last = e;
      if (e.status && e.status < 500 && e.status !== 429) throw e;
      if (i < tries - 1) await sleep(baseMs * 2 ** i);
    }
  }
  throw last;
}
