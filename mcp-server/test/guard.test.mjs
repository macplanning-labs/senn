// _isolated-home.mjs の見張りが本当に効くかを確かめる（DEMO-000116）。
// 守る対象のフォルダを一時フォルダに差し替え、そこへ書き込むテストを子プロセスで動かして、失敗することを見る。
import "./_isolated-home.mjs";

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

test("守る対象のフォルダに書き込んだテストは失敗する", () => {
  const fakeReal = mkdtempSync(join(tmpdir(), "senn-mcp-fake-real-home-"));
  try {
    const dir = join(fakeReal, ".config", "senn-ai-agent");
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "credentials.env"), "SENN_AI_API_KEY=original\n");

    const r = spawnSync(process.execPath, ["--test", join(HERE, "fixtures", "writes-protected.mjs")], {
      env: { PATH: process.env.PATH, MCP_TEST_PROTECTED_HOME: fakeReal },
      encoding: "utf-8",
    });

    assert.notEqual(r.status, 0, "本物の設定を書き換えたのにテストが成功した");
    assert.match(r.stdout + r.stderr, /本物の認証情報のフォルダ/);
    // 見本のテストは実際に書き込んでいる（見張りが「書き込みを見つけた」ことの確認）
    assert.match(readFileSync(join(dir, "credentials.env"), "utf-8"), /fake-client-id/);
  } finally {
    rmSync(fakeReal, { recursive: true, force: true });
  }
});
