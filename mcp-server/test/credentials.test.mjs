// 認証情報の確かめ（DEMO-000116）のテスト。必ず最初に一時 HOME へ切り替える。
import { TEST_HOME, REAL_HOME } from "./_isolated-home.mjs";

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  findPlaceholderCredentials,
  isPlaceholderCredential,
} from "../dist/credentials-guard.js";
import { loadCliConfig } from "../dist/cli/config.js";
import { fetchJson, HttpError } from "../dist/cli/http.js";
import { WipClient } from "../dist/client.js";

const DIST = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");

// 本物の鍵と同じ長さ・文字種の値（実在しない）。
// 乱数のような値にすると、push 前の秘密の検査（gitleaks）が本物の鍵と見なすので、繰り返しで作る。
const REALISTIC = {
  id: "0123456789abcdef".repeat(2) + ".access",
  secret: "0123456789abcdef".repeat(4), // gitleaks:allow（テスト用の実在しない値）
  apiKey: "AbCd1234".repeat(5),
};

function writeCredentials(body) {
  const dir = join(TEST_HOME, ".config", "senn-ai-agent");
  mkdirSync(dir, { recursive: true });
  // 接続先は必須なので、書いていなければ足す
  const withBase = /^SENN_BASE_URL=/m.test(body) ? body : `SENN_BASE_URL=https://senn.example.com\n${body}`;
  writeFileSync(join(dir, "credentials.env"), withBase);
}

test("テストの HOME は一時フォルダで、本物の HOME ではない", () => {
  assert.equal(homedir(), TEST_HOME);
  assert.notEqual(homedir(), REAL_HOME);
});

test("テスト用の値を見つける", () => {
  for (const v of [
    "fake-client-id",
    "fake-client-secret",
    "fake-api-key",
    "test-key",
    "TEST_SECRET",
    "dummy",
    "dummy.value",
    "example-token",
    "sample_key",
    "placeholder",
    "changeme",
    "your-api-key",
    "xxx",
    "<your-client-id>",
    "...",
    "****",
  ]) {
    assert.equal(isPlaceholderCredential(v), true, v);
  }
});

test("本物の鍵の形の値は止めない", () => {
  for (const v of [
    REALISTIC.id,
    REALISTIC.secret,
    REALISTIC.apiKey,
    "testimony1234567890", // test の後に区切りが無ければ普通の文字列
    "fakeroot9a8b7c",
    "deadbeef".repeat(8),
  ]) {
    assert.equal(isPlaceholderCredential(v), false, v);
  }
});

test("空・未設定は対象外（Cloudflare を使わない環境では CF_ACCESS_* が無いのが正しい）", () => {
  assert.deepEqual(
    findPlaceholderCredentials({ A: undefined, B: "", C: "  ", D: REALISTIC.apiKey }),
    []
  );
  assert.deepEqual(findPlaceholderCredentials({ A: "fake-x", B: REALISTIC.id }), ["A"]);
});

test("senn-ai-run: credentials.env がテスト用の値なら止まり、値はエラー文に出さない", () => {
  writeCredentials(
    [
      "SENN_CF_ACCESS_CLIENT_ID=fake-client-id",
      "SENN_CF_ACCESS_CLIENT_SECRET=fake-client-secret",
      "SENN_AI_API_KEY=fake-api-key",
    ].join("\n")
  );
  assert.throws(
    () => loadCliConfig(),
    (err) => {
      assert.match(err.message, /テスト用の値/);
      assert.match(err.message, /SENN_CF_ACCESS_CLIENT_ID/);
      assert.doesNotMatch(err.message, /fake-client/);
      return true;
    }
  );
});

test("senn-ai-run: 本物の形の値なら読み込んでヘッダーにする", () => {
  writeCredentials(
    [
      `SENN_CF_ACCESS_CLIENT_ID=${REALISTIC.id}`,
      `SENN_CF_ACCESS_CLIENT_SECRET="${REALISTIC.secret}"`,
      `SENN_AI_API_KEY=${REALISTIC.apiKey}`,
    ].join("\n")
  );
  const config = loadCliConfig();
  assert.equal(config.headers["CF-Access-Client-Id"], REALISTIC.id);
  assert.equal(config.headers["CF-Access-Client-Secret"], REALISTIC.secret);
  assert.equal(config.headers["X-AI-Api-Key"], REALISTIC.apiKey);
});

test("senn-ai-run: Cloudflare の鍵が無くても止めない（OSS 版など）", () => {
  writeCredentials(`SENN_AI_API_KEY=${REALISTIC.apiKey}\n`);
  const config = loadCliConfig();
  assert.equal(config.headers["CF-Access-Client-Id"], undefined);
  assert.equal(config.headers["X-AI-Api-Key"], REALISTIC.apiKey);
});

test("MCP サーバー: テスト用の値なら起動しない", () => {
  const r = spawnSync(
    process.execPath,
    ["-e", `import(${JSON.stringify(join(DIST, "config.js"))}).then(m => m.loadConfig())`],
    {
      env: {
        PATH: process.env.PATH,
        HOME: TEST_HOME,
        SENN_CF_ACCESS_CLIENT_ID: "fake-client-id",
        SENN_CF_ACCESS_CLIENT_SECRET: REALISTIC.secret,
        SENN_AI_API_KEY: REALISTIC.apiKey,
      },
      encoding: "utf-8",
    }
  );
  assert.equal(r.status, 1);
  assert.match(r.stderr, /テスト用の値/);
  assert.match(r.stderr, /SENN_CF_ACCESS_CLIENT_ID/);
  assert.doesNotMatch(r.stderr, /fake-client-id/);
  assert.doesNotMatch(r.stderr, new RegExp(REALISTIC.secret));
});

// ---- Cloudflare Access の 403 を見分ける ----

const CF_HTML =
  '<!doctype html><html><head><title>Error ・ Cloudflare Access</title></head><body>Forbidden</body></html>';

async function withServer(handler, fn) {
  const server = createServer(handler);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  try {
    return await fn(`http://127.0.0.1:${port}`);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

const cfBlock = (req, res) => {
  res.writeHead(403, { "Content-Type": "text/html" });
  res.end(CF_HTML);
};
const sennJson401 = (req, res) => {
  res.writeHead(401, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ error: "AI用APIキーが無効です" }));
};

test("senn-ai-run: Cloudflare の HTML 403 は原因が分かる文になる", async () => {
  await withServer(cfBlock, async (baseUrl) => {
    await assert.rejects(
      fetchJson("/api/v1/ai-agent/runs/x/start/", {}, { baseUrl, headers: {}, maxRetries: 1 }),
      (err) => {
        assert.ok(err instanceof HttpError);
        assert.equal(err.statusCode, 403);
        assert.match(err.message, /Cloudflare Access に止められました/);
        return true;
      }
    );
  });
});

test("senn-ai-run: SENN 本体の JSON エラーは今までどおり", async () => {
  await withServer(sennJson401, async (baseUrl) => {
    await assert.rejects(
      fetchJson("/api/v1/ai-agent/runs/x/start/", {}, { baseUrl, headers: {}, maxRetries: 1 }),
      (err) => {
        assert.equal(err.statusCode, 401);
        assert.equal(err.message, "HTTP 401");
        assert.deepEqual(err.details, { error: "AI用APIキーが無効です" });
        return true;
      }
    );
  });
});

test("MCP サーバー: Cloudflare の HTML 403 と SENN 本体のエラーを区別する", async () => {
  await withServer(cfBlock, async (baseUrl) => {
    await assert.rejects(new WipClient(baseUrl, {}).listProjects(), /Cloudflare Access に止められました/);
  });
  await withServer(sennJson401, async (baseUrl) => {
    await assert.rejects(new WipClient(baseUrl, {}).listProjects(), /AI用APIキーが無効です/);
  });
});

// ---- 接続先・Cloudflare の値は任意 ----

function runMcpConfig(env) {
  // ほかのテストが書いた credentials.env を読まないよう、空の HOME で動かす
  const home = mkdtempSync(join(TEST_HOME, "mcp-config-"));
  return spawnSync(
    process.execPath,
    ["-e", `import(${JSON.stringify(join(DIST, "config.js"))}).then(m => console.log(JSON.stringify(Object.keys(m.loadConfig().headers))))`],
    { env: { PATH: process.env.PATH, HOME: home, ...env }, encoding: "utf-8" }
  );
}

test("MCP サーバー: 接続先(SENN_BASE_URL)が無ければ、理由を出して止まる(会社の URL を既定にしない)", () => {
  const r = runMcpConfig({ SENN_AI_API_KEY: REALISTIC.apiKey });
  assert.equal(r.status, 1);
  assert.match(r.stderr, /SENN_BASE_URL/);
});

test("MCP サーバー: Cloudflare の値もキーも無くても起動する(端末の接続ができる)", () => {
  const r = runMcpConfig({ SENN_BASE_URL: "https://senn.example.com" });
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(JSON.parse(r.stdout), []);
  assert.match(r.stderr, /senn_connect_account/);
});

test("MCP サーバー: Cloudflare の値が片方だけなら止まる", () => {
  const r = runMcpConfig({ SENN_BASE_URL: "https://senn.example.com", SENN_CF_ACCESS_CLIENT_ID: REALISTIC.id });
  assert.equal(r.status, 1);
});

test("MCP サーバー: http(localhost 以外)なら警告を出す", () => {
  const r = runMcpConfig({ SENN_BASE_URL: "http://senn.example.com", SENN_AI_API_KEY: REALISTIC.apiKey });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, /警告: SENN_BASE_URL が http/);
  const local = runMcpConfig({ SENN_BASE_URL: "http://localhost:8080", SENN_AI_API_KEY: REALISTIC.apiKey });
  assert.doesNotMatch(local.stderr, /警告/);
});

test("senn-ai-run: 接続先は credentials.env からも読む", () => {
  writeCredentials(`SENN_BASE_URL=https://senn.example.com/\nSENN_AI_API_KEY=${REALISTIC.apiKey}\n`);
  assert.equal(loadCliConfig().baseUrl, "https://senn.example.com");
});
