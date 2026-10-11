// 端末の接続(コピー・貼り付けなし)のテスト(DEMO-000192 フェーズ4)。必ず最初に一時 HOME へ切り替える。
import "./_isolated-home.mjs";

import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  applyStatus,
  clearState,
  generateKey,
  hashKey,
  keyPrefix12,
  loadState,
  saveState,
  updateCredentials,
} from "../dist/pairing.js";

function tmp() {
  return mkdtempSync(join(tmpdir(), "senn-pairing-test-"));
}

const OLD = "old_value_for_test_only";

function creds(dir, lines) {
  writeFileSync(join(dir, "credentials.env"), lines.join("\n") + "\n", { mode: 0o600 });
}

function state(over = {}) {
  return {
    code: "ABCD-EFGH",
    deviceSecret: "secret",
    key: generateKey(),
    url: "https://example.test/connect?code=ABCD-EFGH",
    label: "テストMac",
    createdAt: new Date().toISOString(),
    ...over,
  };
}

test("キーは senn_ai_ + 43文字(256bit)で、毎回違う", () => {
  const a = generateKey();
  const b = generateKey();
  assert.match(a, /^senn_ai_[A-Za-z0-9_-]{43}$/);
  assert.notEqual(a, b);
});

test("指紋はキーの SHA-256(小文字16進64文字)で、サーバーの照合と同じ計算", () => {
  const k = "senn_ai_" + "x".repeat(43);
  assert.equal(hashKey(k), createHash("sha256").update(k).digest("hex"));
  assert.match(hashKey(k), /^[0-9a-f]{64}$/);
});

test("先頭12文字は senn_ai_ で始まり、サーバーの形(英数字・_ ・ -)に合う", () => {
  const p = keyPrefix12(generateKey());
  assert.equal(p.length, 12);
  assert.match(p, /^senn_ai_[A-Za-z0-9_-]{4}$/);
});

test("待ちの状態は、権限600で保存され、読み戻せて、消せる", () => {
  const dir = tmp();
  const st = state();
  saveState(dir, st);
  assert.equal(statSync(join(dir, "pairing.json")).mode & 0o777, 0o600);
  assert.deepEqual(loadState(dir), st);
  clearState(dir);
  assert.equal(loadState(dir), null);
});

test("14日より古い待ちの状態は、捨てる", () => {
  const dir = tmp();
  saveState(dir, state({ createdAt: new Date(Date.now() - 15 * 86_400_000).toISOString() }));
  assert.equal(loadState(dir), null);
});

test("credentials.env の SENN_AI_API_KEY だけを書き換え、ほかの行は変えず、バックアップを取る", () => {
  const dir = tmp();
  creds(dir, ["SENN_CF_ACCESS_CLIENT_ID=id-keep", "SENN_AI_API_KEY=" + OLD, "SENN_BASE_URL=https://example.test"]);
  const key = generateKey();
  const r = updateCredentials(dir, key);
  assert.equal(r.written, true);
  const after = readFileSync(join(dir, "credentials.env"), "utf8");
  assert.equal(
    after,
    ["SENN_CF_ACCESS_CLIENT_ID=id-keep", "SENN_AI_API_KEY=" + key, "SENN_BASE_URL=https://example.test", ""].join("\n")
  );
  assert.equal(statSync(join(dir, "credentials.env")).mode & 0o777, 0o600);
  // バックアップには、元の内容が残る(権限600)
  assert.match(readFileSync(r.backup, "utf8"), new RegExp(OLD));
  assert.equal(statSync(r.backup).mode & 0o777, 0o600);
  // 一時ファイルが残らない
  assert.ok(readdirSync(dir).every((f) => !f.includes(".tmp-")));
});

test("export 付きの行も書き換え、行が無ければ末尾に足す", () => {
  const dir = tmp();
  creds(dir, ["export SENN_AI_API_KEY=" + OLD]);
  const k1 = generateKey();
  updateCredentials(dir, k1);
  assert.match(readFileSync(join(dir, "credentials.env"), "utf8"), new RegExp(`^SENN_AI_API_KEY=${k1}$`, "m"));

  const dir2 = tmp();
  creds(dir2, ["SENN_BASE_URL=https://example.test"]);
  const k2 = generateKey();
  updateCredentials(dir2, k2);
  assert.equal(
    readFileSync(join(dir2, "credentials.env"), "utf8"),
    `SENN_BASE_URL=https://example.test\nSENN_AI_API_KEY=${k2}\n`
  );
});

test("credentials.env が無いときは、権限600で新しく作る(接続先も書く)", () => {
  const dir = join(tmp(), "new-dir");
  const key = generateKey();
  const r = updateCredentials(dir, key, new Date(), "https://senn.example.com");
  assert.equal(r.written, true);
  const path = join(dir, "credentials.env");
  assert.equal(statSync(path).mode & 0o777, 0o600);
  assert.equal(statSync(dir).mode & 0o777, 0o700);
  const body = readFileSync(path, "utf8");
  assert.match(body, /^SENN_BASE_URL=https:\/\/senn\.example\.com$/m);
  assert.match(body, new RegExp(`^SENN_AI_API_KEY=${key}$`, "m"));
});

test("承認されたら、キーを保存し、ヘッダを入れ替え、待ちの状態を消す", () => {
  const dir = tmp();
  creds(dir, ["SENN_AI_API_KEY=" + OLD]);
  const st = state();
  saveState(dir, st);
  let set = null;
  const out = applyStatus(dir, st, { status: "approved" }, (k) => (set = k));
  assert.deepEqual(out, { kind: "connected", wrote: true });
  assert.equal(set, st.key);
  assert.match(readFileSync(join(dir, "credentials.env"), "utf8"), new RegExp(`SENN_AI_API_KEY=${st.key}`));
  assert.equal(loadState(dir), null);
});

test("申請中・待ちのあいだは、何も変えない", () => {
  for (const status of ["waiting", "requested"]) {
    const dir = tmp();
    creds(dir, ["SENN_AI_API_KEY=" + OLD]);
    const st = state();
    saveState(dir, st);
    let called = false;
    const out = applyStatus(dir, st, { status }, () => (called = true));
    assert.deepEqual(out, { kind: "pending", status });
    assert.equal(called, false);
    assert.match(readFileSync(join(dir, "credentials.env"), "utf8"), new RegExp(OLD));
    assert.ok(loadState(dir), "待ちの状態は残る");
  }
});

test("却下・失敗・期限切れのときは、キーを保存せず、待ちの状態を消す", () => {
  for (const status of ["rejected", "failed", "expired"]) {
    const dir = tmp();
    creds(dir, ["SENN_AI_API_KEY=" + OLD]);
    const st = state();
    saveState(dir, st);
    let called = false;
    const out = applyStatus(dir, st, { status, message: "理由" }, () => (called = true));
    assert.deepEqual(out, { kind: "closed", status, message: "理由" });
    assert.equal(called, false, "ヘッダは入れ替えない");
    assert.match(readFileSync(join(dir, "credentials.env"), "utf8"), new RegExp(OLD), "credentials.env は変えない");
    assert.equal(loadState(dir), null);
  }
});
