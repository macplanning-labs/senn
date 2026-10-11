// Wiki の添付に使うローカルファイルの制限のテスト。必ず最初に一時 HOME へ切り替える。
import "./_isolated-home.mjs";

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { realpathSync } from "node:fs";
import { checkUploadPath, uploadOptionsFromEnv } from "../dist/upload-guard.js";

function workspace() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "senn-upload-")));
  writeFileSync(join(root, "spec.pdf"), "pdf");
  mkdirSync(join(root, ".ssh"));
  writeFileSync(join(root, ".ssh", "notes.txt"), "x");
  writeFileSync(join(root, ".env"), "SECRET=x");
  writeFileSync(join(root, "server.pem"), "x");
  writeFileSync(join(root, "credentials.env"), "x");
  mkdirSync(join(root, "docs"));
  writeFileSync(join(root, "docs", "big.bin"), Buffer.alloc(2 * 1024 * 1024));
  return root;
}

test("作業中のフォルダの下の、普通のファイルは添付できる", async () => {
  const root = workspace();
  const r = await checkUploadPath(join(root, "spec.pdf"), { roots: [root] });
  assert.equal(r.ok, true);
  assert.equal(r.filename, "spec.pdf");
});

test("置き場所の外・相対パスは断る", async () => {
  const root = workspace();
  const other = workspace();
  assert.equal((await checkUploadPath(join(other, "spec.pdf"), { roots: [root] })).ok, false);
  assert.equal((await checkUploadPath("spec.pdf", { roots: [root] })).ok, false);
  assert.equal((await checkUploadPath(join(root, "docs", "..", "..", "x"), { roots: [root] })).ok, false);
});

test("「.」で始まるフォルダ・ファイル、鍵・認証情報らしい名前は断る", async () => {
  const root = workspace();
  for (const p of [join(root, ".ssh", "notes.txt"), join(root, ".env"), join(root, "server.pem"), join(root, "credentials.env")]) {
    const r = await checkUploadPath(p, { roots: [root] });
    assert.equal(r.ok, false, p);
  }
});

test("置き場所の中のシンボリックリンクから、外のファイルは送れない", async () => {
  const root = workspace();
  const outside = workspace();
  symlinkSync(join(outside, "spec.pdf"), join(root, "link.pdf"));
  const r = await checkUploadPath(join(root, "link.pdf"), { roots: [root] });
  assert.equal(r.ok, false);
});

test("大きすぎるファイル・フォルダは断る", async () => {
  const root = workspace();
  assert.equal((await checkUploadPath(join(root, "docs", "big.bin"), { roots: [root], maxBytes: 1024 * 1024 })).ok, false);
  assert.equal((await checkUploadPath(join(root, "docs"), { roots: [root] })).ok, false);
});

test("置き場所の既定: 起動したフォルダ。/ やホームそのものなら、どこも送れない", async () => {
  assert.deepEqual(uploadOptionsFromEnv({}, "/work/repo").roots, ["/work/repo"]);
  assert.deepEqual(uploadOptionsFromEnv({}, "/").roots, []);
  assert.deepEqual(uploadOptionsFromEnv({}, homedir()).roots, []);
  assert.deepEqual(uploadOptionsFromEnv({ SENN_UPLOAD_ROOTS: "/a:/b" }, "/").roots, ["/a", "/b"]);
  const r = await checkUploadPath("/etc/hosts", { roots: [] });
  assert.equal(r.ok, false);
  assert.match(r.reason, /SENN_UPLOAD_ROOTS/);
});
