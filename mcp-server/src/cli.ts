#!/usr/bin/env node
import { executeCommand } from "./cli/commands.js";
import { ArgumentError } from "./cli/args.js";

async function main() {
  const argv = process.argv.slice(2);

  if (argv.length === 0) {
    printUsage();
    process.exit(2);
  }

  const command = argv[0];

  try {
    switch (command) {
      case "start":
        await executeCommand("start", argv.slice(1));
        break;
      case "finish":
        await executeCommand("finish", argv.slice(1));
        break;
      case "error":
        await executeCommand("error", argv.slice(1));
        break;
      default:
        console.error(`エラー: 不明なコマンド '${command}'`);
        printUsage();
        process.exit(2);
    }
  } catch (err) {
    // 引数エラー（ArgumentError）は exit code 2
    if (err instanceof ArgumentError) {
      console.error(`エラー: ${err.message}`);
      printUsage();
      process.exit(2);
    }

    const message =
      err instanceof Error ? err.message : String(err);
    console.error(`エラー: ${message}`);
    process.exit(1);
  }
}

function printUsage() {
  console.error(`使い方:
  senn-ai-run start  <ticket_key> [--run <uuid>] [--tool <tool>] [--model <model>] [--trigger <type>]
  senn-ai-run finish <run_uuid> --result <result> [--summary <text>] [--summary-file <path>]
  senn-ai-run error  <run_uuid> --detail <text> [--ticket <ticket_key>]

例:
  senn-ai-run start PROJ-000123
  senn-ai-run finish abc-123-def --result resolved --summary "実装完了"
  senn-ai-run error abc-123-def --detail "ネットワークエラー"`);
}

main().catch((err) => {
  const message =
    err instanceof Error ? err.message : String(err);
  console.error(`予期しないエラー: ${message}`);
  process.exit(1);
});
