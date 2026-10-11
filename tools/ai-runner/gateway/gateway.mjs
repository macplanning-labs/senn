// gateway.mjs — AI のコンテナと Anthropic の間に立つ中継役(認証を差し込むゲートウェイ)。
//
// 目的: AI(claude -p)が動くコンテナに、本物の Claude のトークンを渡さない。
//   - 本物のトークンを持つのは、この中継役だけ(中継役の環境は、AI からは見えない)
//   - AI のコンテナには、偽のトークンと「ここに送る」設定(ANTHROPIC_BASE_URL)だけを渡す
//   - 中継役は、AI から来た認証情報を捨てて本物のトークンに差し替え、固定の宛先(api.anthropic.com)にだけ送る
//   - Anthropic 側で動く道具(Web の取得・検索、MCP の接続、コードの実行など)を使う要求は断る。
//     AI が操られて、Anthropic のサーバー経由でデータを外へ送らせる抜け道になるため
//
// ログには、メソッド・パス・状態だけを出す(ヘッダー・本文・トークンは出さない)。
// 依存パッケージなし(Node.js 20 以上)。ランナーが実行のたびに、使い捨てのコンテナで起動する。

import http from 'node:http';
import https from 'node:https';
import { fileURLToPath } from 'node:url';

/** 宛先は固定(要求のホスト名や設定からは変えさせない) */
export const UPSTREAM = 'https://api.anthropic.com';

/** 通してよい要求(メソッドとパス。クエリは見ない) */
const ALLOWED = [
  ['POST', /^\/v1\/messages$/],
  ['POST', /^\/v1\/messages\/count_tokens$/],
  ['GET', /^\/v1\/models(\/[A-Za-z0-9._-]+)?$/],
];

/** 本文を調べる要求(道具の指定を含みうるもの) */
const BODY_CHECKED = /^\/v1\/messages(\/count_tokens)?$/;

/** 転送してよいヘッダー(認証・Cookie など、ほかは捨てる) */
const FORWARD_HEADERS = new Set([
  'content-type', 'accept', 'accept-encoding', 'user-agent',
  'anthropic-version', 'anthropic-beta', 'x-app',
]);

/** 本文の上限(バイト) */
const MAX_BODY = 32 * 1024 * 1024;

/**
 * 道具の指定が、手元(Claude Code)で実行されるものだけかを確かめる。
 * - type の無い道具・custom: Claude Code が自分で実行する道具(Read・Edit・Bash など)
 * - bash_* / text_editor_* / computer_* / memory_*: Anthropic が定義するが、実行するのは手元
 * それ以外(web_search_* / web_fetch_* / code_execution_* / mcp_* など)は Anthropic のサーバーで動くので断る。
 * @returns {string|null} 断る理由(通してよければ null)
 */
export function rejectReason(body) {
  if (body === null || typeof body !== 'object' || Array.isArray(body)) return '本文が JSON のオブジェクトではありません';
  if ('mcp_servers' in body) return 'MCP サーバーの指定は使えません';
  if ('container' in body) return 'コンテナ(サーバー側の実行環境)の指定は使えません';
  const tools = body.tools ?? [];
  if (!Array.isArray(tools)) return '道具の指定の形式が正しくありません';
  for (const t of tools) {
    const type = t && typeof t === 'object' ? t.type : undefined;
    if (type === undefined || type === 'custom') continue;
    if (typeof type === 'string' && /^(bash|text_editor|computer|memory)_\d/.test(type)) continue;
    return `サーバー側で動く道具は使えません(${String(type).slice(0, 40)})`;
  }
  return null;
}

/**
 * @param {object} p
 * @param {string} p.token 本物のトークン
 * @param {string} [p.upstream] テストでだけ差し替える(本番は UPSTREAM 固定)
 * @param {(line: string) => void} [p.log]
 */
export function createGateway({ token, upstream = UPSTREAM, log = (l) => console.log(l) }) {
  if (!token) throw new Error('CLAUDE_CODE_OAUTH_TOKEN が設定されていません');
  const up = new URL(upstream);
  const client = up.protocol === 'https:' ? https : http;

  const deny = (res, status, msg, method, pathname) => {
    log(`拒否 ${status} ${method} ${pathname} ${msg}`);
    res.writeHead(status, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ type: 'error', error: { type: 'senn_gateway_denied', message: msg } }));
  };

  return http.createServer((req, res) => {
    const url = new URL(req.url ?? '/', 'http://gateway');
    const { pathname } = url;
    const method = req.method ?? 'GET';
    if (!ALLOWED.some(([m, re]) => m === method && re.test(pathname))) {
      req.resume();
      return deny(res, 403, '許可されていない宛先です', method, pathname);
    }

    const chunks = [];
    let size = 0;
    let tooLarge = false;
    req.on('data', (c) => {
      size += c.length;
      if (size > MAX_BODY) { tooLarge = true; return; }
      chunks.push(c);
    });
    req.on('end', () => {
      if (tooLarge) return deny(res, 413, '本文が大きすぎます', method, pathname);
      const body = Buffer.concat(chunks);
      if (method === 'POST' && BODY_CHECKED.test(pathname)) {
        let parsed;
        try { parsed = JSON.parse(body.toString('utf8')); } catch { return deny(res, 400, '本文が JSON ではありません', method, pathname); }
        const why = rejectReason(parsed);
        if (why) return deny(res, 403, why, method, pathname);
      }

      const headers = {};
      for (const [k, v] of Object.entries(req.headers)) {
        if (FORWARD_HEADERS.has(k.toLowerCase()) && v !== undefined) headers[k] = v;
      }
      headers.authorization = `Bearer ${token}`;
      headers.host = up.host;
      if (body.length) headers['content-length'] = String(body.length);

      const upReq = client.request({
        protocol: up.protocol, hostname: up.hostname, port: up.port || undefined,
        method, path: pathname + url.search, headers,
      }, (upRes) => {
        log(`転送 ${upRes.statusCode} ${method} ${pathname}`);
        const out = { ...upRes.headers };
        delete out['set-cookie'];
        res.writeHead(upRes.statusCode ?? 502, out);
        upRes.pipe(res); // ストリーミング(SSE)もそのまま流す
      });
      upReq.on('error', (e) => {
        log(`失敗 502 ${method} ${pathname} ${e.code ?? 'error'}`);
        if (!res.headersSent) res.writeHead(502, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ type: 'error', error: { type: 'senn_gateway_upstream', message: 'Anthropic に接続できませんでした' } }));
      });
      upReq.end(body);
    });
  });
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const port = Number(process.env.GATEWAY_PORT || 8080);
  const token = process.env.CLAUDE_CODE_OAUTH_TOKEN || '';
  // トークンは読み込んだら環境から消す(このプロセスの子には渡さない)
  delete process.env.CLAUDE_CODE_OAUTH_TOKEN;
  const server = createGateway({ token });
  server.listen(port, '0.0.0.0', () => console.log(`中継役を開始しました(:${port} → ${UPSTREAM})`));
  for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => server.close(() => process.exit(0)));
}
