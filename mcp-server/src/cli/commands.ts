import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import {
  parseStartArgs,
  parseFinishArgs,
  parseErrorArgs,
  ArgumentError,
} from "./args.js";
import { loadCliConfig } from "./config.js";
import { fetchJson, HttpError } from "./http.js";

interface StartResponse {
  run_uuid: string;
  ticket_key: string;
  ticket_status?: string;
  created?: boolean;
  started_at?: string;
}

interface FinishResponse {
  run_uuid: string;
  result: string;
  ended_at?: string;
  ticket_status?: string;
}

interface ErrorResponse {
  run_uuid: string;
  recorded: boolean;
}

/** テキストを最大2000字に切り詰める */
function truncate(text: string, maxLength: number = 2000): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength - 4) + "…(省略)";
}

/** エラーレスポンスをサーバーに報告（ベストエフォート） */
async function reportError(
  runUuid: string,
  detail: string,
  config: ReturnType<typeof loadCliConfig>,
  ticketKey?: string
): Promise<void> {
  try {
    const body: Record<string, unknown> = { detail: truncate(detail) };
    // run がまだ作られていない(start が失敗した)場合でも記録できるように ticket_key を添える
    if (ticketKey) body.ticket_key = ticketKey;
    await fetchJson<ErrorResponse>(
      `/api/v1/ai-agent/runs/${runUuid}/error/`,
      body,
      config
    );
  } catch {
    // ベストエフォート：失敗しても無視
  }
}

/** サーバーのエラーメッセージを解析 */
function extractServerError(err: unknown): string {
  if (err instanceof HttpError) {
    const details = err.details as Record<string, unknown> | null;
    if (details && typeof details === "object") {
      if ("detail" in details && typeof details.detail === "string") {
        return details.detail;
      }
      if ("message" in details && typeof details.message === "string") {
        return details.message;
      }
    }
    return err.statusCode ? `HTTP ${err.statusCode}` : err.message;
  }
  if (err instanceof Error) {
    return err.message;
  }
  return String(err);
}

export async function executeCommand(
  command: string,
  argv: string[]
): Promise<void> {
  const config = loadCliConfig();
  // 失敗時の error 報告に使う(start は自動生成した run_uuid もここに残す)
  let reportTarget: { runUuid: string; ticketKey?: string } | null = null;

  try {
    switch (command) {
      case "start": {
        const args = parseStartArgs(argv);
        const runUuid = args.run || randomUUID();
        reportTarget = { runUuid, ticketKey: args.ticketKey };
        const body: Record<string, unknown> = {
          ticket_key: args.ticketKey,
        };
        if (args.tool) body.tool = args.tool;
        if (args.model) body.model = args.model;
        if (args.trigger) body.trigger_type = args.trigger;

        const response = await fetchJson<StartResponse>(
          `/api/v1/ai-agent/runs/${runUuid}/start/`,
          body,
          config
        );

        // 成功時は JSON 1行を標準出力に
        console.log(
          JSON.stringify({
            run_uuid: response.run_uuid,
            ticket_key: response.ticket_key,
            ticket_status: response.ticket_status,
            created: response.created !== false,
          })
        );
        break;
      }

      case "finish": {
        const args = parseFinishArgs(argv);
        reportTarget = { runUuid: args.runUuid };

        // summaryを読み込む
        let summary = args.summary;
        if (args.summaryFile) {
          try {
            summary = readFileSync(args.summaryFile, "utf-8");
          } catch (err) {
            throw new Error(
              `summary ファイルを読み込めません: ${args.summaryFile}`
            );
          }
        }

        const body: Record<string, unknown> = {
          result: args.result,
        };
        if (summary) {
          body.summary = truncate(summary);
        }

        const finished = await fetchJson<FinishResponse>(
          `/api/v1/ai-agent/runs/${args.runUuid}/finish/`,
          body,
          config
        );

        // 成功時は JSON 1行(AI が結果を確認できるように)
        console.log(
          JSON.stringify({
            run_uuid: finished.run_uuid,
            result: finished.result,
            ticket_status: finished.ticket_status,
          })
        );
        break;
      }

      case "error": {
        const args = parseErrorArgs(argv);
        const body: Record<string, unknown> = {
          detail: truncate(args.detail),
        };
        if (args.ticket) {
          body.ticket_key = args.ticket;
        }

        await fetchJson<ErrorResponse>(
          `/api/v1/ai-agent/runs/${args.runUuid}/error/`,
          body,
          config
        );

        // 成功時は標準出力に表示しない
        break;
      }

      default:
        throw new Error(`不明なコマンド: ${command}`);
    }
  } catch (err) {
    // 引数エラーは throw し直す
    if (err instanceof ArgumentError) {
      throw err;
    }

    // エラーメッセージを標準エラーに出す
    const errorMsg = extractServerError(err);
    console.error(
      `${command} に失敗しました: ${errorMsg}`
    );

    // start/finish が 4xx/5xx(サーバーが応答した失敗)のときだけ、error エンドポイントに報告する。
    // ネットワーク断では送れないので送らない(サーバー側の見張りが拾う)。
    if (
      (command === "start" || command === "finish") &&
      err instanceof HttpError &&
      reportTarget
    ) {
      const detail = `${command} に失敗しました: ${errorMsg}`;
      await reportError(
        reportTarget.runUuid,
        detail,
        config,
        reportTarget.ticketKey
      );
    }

    process.exit(1);
  }
}
