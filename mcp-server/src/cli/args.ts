import { parseArgs as nodParseArgs } from "node:util";

export class ArgumentError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ArgumentError";
  }
}

export interface StartArgs {
  ticketKey: string;
  run?: string;
  tool?: string;
  model?: string;
  trigger?: string;
}

export interface FinishArgs {
  runUuid: string;
  result: "resolved" | "blocked" | "aborted";
  summary?: string;
  summaryFile?: string;
}

export interface ErrorArgs {
  runUuid: string;
  detail: string;
  ticket?: string;
}

export function parseStartArgs(argv: string[]): StartArgs {
  if (argv.length === 0) {
    throw new ArgumentError("start コマンドには <ticket_key> が必須です");
  }

  const ticketKey = argv[0];

  const parsed = nodParseArgs({
    args: argv.slice(1),
    options: {
      run: { type: "string" },
      tool: { type: "string" },
      model: { type: "string" },
      trigger: { type: "string" },
    },
    allowPositionals: false,
  });

  return {
    ticketKey,
    run: parsed.values.run,
    tool: parsed.values.tool,
    model: parsed.values.model,
    trigger: parsed.values.trigger,
  };
}

export function parseFinishArgs(argv: string[]): FinishArgs {
  if (argv.length === 0) {
    throw new ArgumentError("finish コマンドには <run_uuid> が必須です");
  }

  const runUuid = argv[0];

  const parsed = nodParseArgs({
    args: argv.slice(1),
    options: {
      result: { type: "string" },
      summary: { type: "string" },
      "summary-file": { type: "string" },
    },
    allowPositionals: false,
  });

  const result = parsed.values.result;
  if (!result || !["resolved", "blocked", "aborted"].includes(result)) {
    throw new ArgumentError(
      "finish コマンドには --result resolved|blocked|aborted が必須です"
    );
  }

  return {
    runUuid,
    result: result as "resolved" | "blocked" | "aborted",
    summary: parsed.values.summary,
    summaryFile: parsed.values["summary-file"],
  };
}

export function parseErrorArgs(argv: string[]): ErrorArgs {
  if (argv.length === 0) {
    throw new ArgumentError("error コマンドには <run_uuid> が必須です");
  }

  const runUuid = argv[0];

  const parsed = nodParseArgs({
    args: argv.slice(1),
    options: {
      detail: { type: "string" },
      ticket: { type: "string" },
    },
    allowPositionals: false,
  });

  const detail = parsed.values.detail;
  if (!detail) {
    throw new ArgumentError("error コマンドには --detail が必須です");
  }

  return {
    runUuid,
    detail,
    ticket: parsed.values.ticket,
  };
}
