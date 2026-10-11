import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
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

export function registerDependencyTools(server: McpServer, client: WipClient) {
  // ============ senn_list_ticket_dependencies ============
  const listTicketDependenciesShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: SENN-000069）"),
  };

  server.tool(
    "senn_list_ticket_dependencies",
    "チケットの依存関係一覧を取得します。各行の fromTask（先に終わらせる側）→ toTask（後に続く側）で、dependencyType が blocks なら fromTask が終わらないと toTask に進めません（relates_to は順番の無い関連）。",
    listTicketDependenciesShape,
    async (args) => {
      try {
        const dependencies = await client.listTicketDependencies(
          args.ticket_key
        );
        return {
          content: [
            { type: "text", text: JSON.stringify(dependencies, null, 2) },
          ],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_add_ticket_dependency ============
  const addTicketDependencyShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe(
        "先に終わらせる側のチケットキー（例: SENN-000068）。blocks のとき、このチケットが終わらないと to_task_key に進めない"
      ),
    to_task_key: z
      .string()
      .min(1)
      .describe(
        "後に続く側のチケットキー（例: SENN-000069）。blocks のとき、ticket_key が終わるのを待つ側"
      ),
    dependency_type: z
      .enum(["blocks", "relates_to"])
      .optional()
      .describe(
        "依存の種類。blocks（既定）= ticket_key が終わらないと to_task_key に進めない（ticket_key が先）。relates_to = 順番の無い「関連」"
      ),
  };

  server.tool(
    "senn_add_ticket_dependency",
    "チケット間に依存関係を追加します。向き: ticket_key が「先に終わらせる側」、to_task_key が「後に続く側」です。例: 設計 SENN-1 が終わらないと実装 SENN-2 に進めない → ticket_key=SENN-1, to_task_key=SENN-2, dependency_type=blocks。画面では SENN-1 に「ブロックしている」、SENN-2 に「ブロックされている」と出ます。自己参照・重複・循環はSENN側が400エラーで弾きます。複数のチケットと依存をまとめて作るときは senn_bulk_create_tickets を使ってください。",
    addTicketDependencyShape,
    async (args) => {
      try {
        const dependency = await client.addTicketDependency(
          args.ticket_key,
          {
            to_task_key: args.to_task_key,
            dependency_type: args.dependency_type,
          }
        );
        return {
          content: [
            { type: "text", text: JSON.stringify(dependency, null, 2) },
          ],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_delete_ticket_dependency ============
  const deleteTicketDependencyShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: SENN-000069）"),
    dependency_id: z
      .number()
      .int()
      .describe("依存関係ID（senn_list_ticket_dependenciesで取得）"),
  };

  server.tool(
    "senn_delete_ticket_dependency",
    "チケット間の依存関係を削除します。依存関係IDはsenn_list_ticket_dependenciesで確認できます。",
    deleteTicketDependencyShape,
    async (args) => {
      try {
        const result = await client.deleteTicketDependency(
          args.ticket_key,
          args.dependency_id
        );
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_get_project_dependency_graph ============
  const getProjectDependencyGraphShape = {
    project_prefix: z
      .string()
      .min(1)
      .describe("プロジェクトprefix（例: SENN）"),
  };

  server.tool(
    "senn_get_project_dependency_graph",
    "プロジェクト全体の依存関係グラフを取得します。ノード（チケット）とエッジ（依存関係）が一括で返されるため、プロジェクト内の全チケットの依存関係構造を把握する際に使います。",
    getProjectDependencyGraphShape,
    async (args) => {
      try {
        const graph = await client.getProjectDependencyGraph(
          args.project_prefix
        );
        return {
          content: [{ type: "text", text: JSON.stringify(graph, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );
}
