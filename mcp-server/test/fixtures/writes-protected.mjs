// guard.test.mjs から子プロセスで動かす「やってはいけないテスト」の見本。
// 守る対象のフォルダ（差し替えた一時フォルダ）の credentials.env を上書きする。
//
// 差し替え先（MCP_TEST_PROTECTED_HOME）が無いときは何もしない。
// `node --test` を引数なしで実行すると test/ の下のファイルが全部動くため、
// その場合に本物の credentials.env を上書きしないようにする。
import { test } from "node:test";

const protectedHome = process.env.MCP_TEST_PROTECTED_HOME;

if (!protectedHome) {
  test("見本のテスト（guard.test.mjs からだけ動かす）", { skip: true }, () => {});
} else {
  const { REAL_HOME } = await import("../_isolated-home.mjs");
  const { writeFileSync } = await import("node:fs");
  const { join } = await import("node:path");
  if (REAL_HOME !== protectedHome) throw new Error("差し替え先が使われていません。中止します。");

  test("本物の credentials.env を上書きしてしまうテスト", () => {
    writeFileSync(
      join(REAL_HOME, ".config", "senn-ai-agent", "credentials.env"),
      "SENN_CF_ACCESS_CLIENT_ID=fake-client-id\n"
    );
  });
}
