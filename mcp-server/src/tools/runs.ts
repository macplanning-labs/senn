import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { randomUUID } from "node:crypto";
import { z } from "zod";
import { WipClient, WipApiError } from "../client.js";

function toErrorResult(e: unknown) {
  if (e instanceof WipApiError) {
    return {
      isError: true,
      content: [
        {
          type: "text" as const,
          text: `SENNエラー(HTTP ${e.status}): ${e.message}`,
        },
      ],
    };
  }
  throw e;
}

export function registerRunTools(server: McpServer, client: WipClient) {
  // ============ senn_ai_run_start ============
  const aiRunStartShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: PROJ-123）"),
    run_uuid: z
      .string()
      .uuid()
      .optional()
      .describe(
        "実行UUID（任意）。SENN の画面でプロンプトを作ったときに書かれている UUID があれば、それを渡す。省略すると新しく作り、結果で返す（終了のときに使う）"
      ),
    tool: z
      .enum(["claude_code", "cursor"])
      .optional()
      .describe("使用ツール（claude_code | cursor、任意。既定 claude_code）"),
    model: z
      .string()
      .optional()
      .describe("AIモデル（任意。例: claude-opus-5-5）"),
  };

  server.tool(
    "senn_ai_run_start",
    "チケットの作業を始めるときに呼びます（MCP で作業するときも必ず呼ぶ）。SENN の「実行」機能から始めたときと同じ扱いになり、チケットの着手日（開始日が空なら今日）・実行の記録・レポートに反映されます。結果の run_uuid を、作業の終わりに senn_ai_run_finish へ渡してください。同じ run_uuid での再呼び出しは何もしません。",
    aiRunStartShape,
    async (args) => {
      try {
        const runUuid = args.run_uuid ?? randomUUID();
        const result = await client.aiRunStart({
          ticket_key: args.ticket_key,
          run_uuid: runUuid,
          tool: args.tool,
          model: args.model,
          // 画面のプロンプトの UUID が無い = MCP から直接始めた
          trigger_type: args.run_uuid ? undefined : "api",
        });
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_ai_run_finish ============
  const tokenCount = z.number().int().min(0).optional();
  const aiRunFinishShape = {
    run_uuid: z
      .string()
      .uuid()
      .describe("実行UUID（senn_ai_run_start の結果の run_uuid）"),
    result: z
      .enum(["resolved", "blocked", "aborted"])
      .describe("結果（resolved=完了 | blocked=人の判断待ちで止まった | aborted=中止）"),
    summary: z
      .string()
      .max(2000)
      .optional()
      .describe("作業サマリー（任意、2000字以内）"),
    input_tokens: tokenCount.describe("使った入力トークン数（任意。わかる範囲で）"),
    output_tokens: tokenCount.describe("使った出力トークン数（任意）"),
    cache_read_tokens: tokenCount.describe("キャッシュから読んだトークン数（任意）"),
    cache_creation_tokens: tokenCount.describe("キャッシュに書いたトークン数（任意）"),
    usd_equivalent: z
      .number()
      .min(0)
      .optional()
      .describe("API料金に換算した金額（米ドル、任意）"),
  };

  server.tool(
    "senn_ai_run_finish",
    "作業を終える・中断するときに呼びます。結果と、使ったトークンの量（わかる範囲で）を記録し、レポートとトークン量の集計に反映されます。resolvedにしてもチケットは自動的に閉じません。マージ・本番反映は人が行います。",
    aiRunFinishShape,
    async (args) => {
      try {
        const result = await client.aiRunFinish({
          run_uuid: args.run_uuid,
          result: args.result,
          summary: args.summary,
          input_tokens: args.input_tokens,
          output_tokens: args.output_tokens,
          cache_read_tokens: args.cache_read_tokens,
          cache_creation_tokens: args.cache_creation_tokens,
          usd_equivalent: args.usd_equivalent,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_ai_run_error ============
  const aiRunErrorShape = {
    run_uuid: z
      .string()
      .uuid()
      .describe("実行UUID（UUID形式）"),
    detail: z
      .string()
      .min(1)
      .max(2000)
      .describe("エラー詳細（1〜2000字）"),
    ticket_key: z
      .string()
      .optional()
      .describe("チケットキー（任意。run作成失敗時に必要）"),
  };

  server.tool(
    "senn_ai_run_error",
    "AIが実行中にエラーに遭遇した際の記録を作成します。run_uuidとdetailは必須。ticket_keyは任意。",
    aiRunErrorShape,
    async (args) => {
      try {
        const result = await client.aiRunError({
          run_uuid: args.run_uuid,
          detail: args.detail,
          ticket_key: args.ticket_key,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );
}
