// read-guard.mjs — Claude Code の PreToolUse フック(Read)。
// 大きいファイルを丸ごと読んでトークンを浪費しないよう、範囲指定(offset/limit)なしの読み込みを拒否する。
// 終了コード 2 = 拒否(標準エラーの内容が AI に返る)。それ以外は許可。
import fs from 'node:fs';

const MAX_BYTES = 120_000;
let raw = '';
process.stdin.on('data', (d) => (raw += d));
process.stdin.on('end', () => {
  try {
    const ev = JSON.parse(raw);
    const input = ev.tool_input ?? {};
    const file = input.file_path;
    if (typeof file !== 'string') process.exit(0);
    const ranged = Number.isInteger(input.limit) && input.limit > 0;
    const size = fs.statSync(file).size;
    if (size > MAX_BYTES && !ranged) {
      process.stderr.write(`このファイルは大きい(${Math.round(size / 1024)}KB)ため、丸ごとは読めません。grep で場所を探し、offset と limit を指定して必要な範囲だけ読んでください。\n`);
      process.exit(2);
    }
  } catch {
    // 判断できないときは許可(フックの不具合で作業を止めない)
  }
  process.exit(0);
});
