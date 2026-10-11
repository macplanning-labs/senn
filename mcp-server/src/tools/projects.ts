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

export function registerProjectTools(server: McpServer, client: WipClient) {
  // ============ senn_list_projects ============
  server.tool(
    "senn_list_projects",
    "SENNの全プロジェクト一覧を取得します。プロジェクト内のチケット・サイクルなどを操作する際に、プロジェクトprefixが必要になるため、まずこのツールで全プロジェクトを確認してください。",
    async () => {
      try {
        const projects = await client.listProjects();
        return {
          content: [{ type: "text", text: JSON.stringify(projects, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_create_project ============
  const createProjectShape = {
    name: z
      .string()
      .min(1)
      .describe("プロジェクト名（例: AIDA, STACKCHAN）"),
    prefix: z
      .string()
      .min(1)
      .describe(
        "プロジェクト識別子（英字のみ、チケットキーの前置きになる例: SENN, AIDA）"
      ),
    description: z.string().optional().describe("プロジェクトの説明（任意）"),
    team_ids: z
      .array(z.number().int())
      .min(1)
      .describe("参加チーム ID（1つ以上必須。チーム IDは senn_list_teams で確認）"),
  };

  server.tool(
    "senn_create_project",
    "新規プロジェクトをSENNに作成します。prefixは一意でなければならず、重複した場合は409エラーが返ります。",
    createProjectShape,
    async (args) => {
      try {
        const project = await client.createProject({
          name: args.name,
          prefix: args.prefix,
          description: args.description,
          team_ids: args.team_ids,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(project, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );
}
