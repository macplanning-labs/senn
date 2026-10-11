// テストを本物の認証情報から切り離す（DEMO-000116）。
//
// 2026-10-07、senn-ai-run のテストで ~/.config/senn-ai-agent/credentials.env を
// 偽の値で上書きし、翌日 AI→SENN の操作が全部止まった。
// このファイルを各テストの最初に import すると:
//   1. HOME を使い捨ての一時フォルダに差し替える（以降の homedir() はそこを指す）
//   2. 呼び出し元のシェルにある SENN_* / WIP_* の環境変数を消す（本物の値を使わない）
//   3. 本物の設定フォルダの中身を記録し、テストの後で変わっていないことを確かめる
// 本物の HOME は環境変数ではなく OS のユーザー情報から取るので、HOME を差し替えても見失わない。

import { after } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir, userInfo } from "node:os";
import { join } from "node:path";

// MCP_TEST_PROTECTED_HOME は、この仕組み自体のテスト（guard.test.mjs）だけが使う。
export const REAL_HOME = process.env.MCP_TEST_PROTECTED_HOME || userInfo().homedir;

/** 本物の認証情報が置かれているフォルダ。テストはここに触れてはいけない。 */
export const PROTECTED_DIRS = [
  ".config/senn-ai-agent",
  ".config/wip-ai-agent",
  ".senn-ai-runner",
  ".config/cloudflare-readonly",
].map((p) => join(REAL_HOME, p));

/** フォルダの中身をハッシュで記録する（値そのものは持たない）。 */
function snapshot() {
  const result = {};
  for (const dir of PROTECTED_DIRS) {
    if (!existsSync(dir)) {
      result[dir] = null;
      continue;
    }
    const files = {};
    for (const name of readdirSync(dir).sort()) {
      const p = join(dir, name);
      const st = statSync(p);
      if (!st.isFile()) continue;
      files[name] = `${st.mtimeMs}:${createHash("sha256").update(readFileSync(p)).digest("hex")}`;
    }
    result[dir] = files;
  }
  return result;
}

const before = snapshot();

export const TEST_HOME = mkdtempSync(join(tmpdir(), "senn-mcp-test-home-"));
process.env.HOME = TEST_HOME;
process.env.USERPROFILE = TEST_HOME;
for (const key of Object.keys(process.env)) {
  if (/^(SENN|WIP)_/.test(key) || key === "CLAUDE_CODE_REMOTE_SESSION_ID") {
    delete process.env[key];
  }
}

if (TEST_HOME === REAL_HOME || TEST_HOME.startsWith(`${REAL_HOME}/.config`)) {
  throw new Error("テスト用の HOME が本物の HOME を指しています。テストを中止します。");
}

after(() => {
  rmSync(TEST_HOME, { recursive: true, force: true });
  assert.deepEqual(
    snapshot(),
    before,
    "テスト中に本物の認証情報のフォルダ（~/.config/senn-ai-agent など）が変わりました。" +
      "テストは TEST_HOME の下だけを使ってください（DEMO-000116）。"
  );
});
