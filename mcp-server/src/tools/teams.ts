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

export function registerTeamTools(server: McpServer, client: WipClient) {
  // ============ senn_list_teams ============
  server.tool(
    "senn_list_teams",
    "SENNのチーム一覧を取得します。プロジェクト作成時の team_ids やチケットの team_id に使うチームIDを確認するときに使います。",
    async () => {
      try {
        const teams = await client.listTeams();
        return {
          content: [{ type: "text", text: JSON.stringify(teams, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_create_team ============
  const createTeamShape = {
    name: z.string().min(1).describe("チーム名（例: Wipアプリ開発チーム）"),
    slug: z
      .string()
      .optional()
      .describe(
        "スラッグ（英小文字・ハイフン推奨。省略時は name から自動生成。日本語名のみだと team になるため明示推奨）"
      ),
    description: z.string().optional().describe("説明（任意）"),
    icon: z.string().optional().describe("アイコン絵文字（任意。既定 👥）"),
    color: z
      .string()
      .optional()
      .describe("カラーコード（任意。既定 #6366f1）"),
    is_active: z
      .boolean()
      .optional()
      .describe("有効フラグ（省略時 true）"),
    prefix: z
      .string()
      .regex(/^[A-Za-z0-9]{1,20}$/)
      .optional()
      .describe(
        "チケット番号の頭（英数字1〜20文字。大文字にそろえる。例: AUT → AUT-000001）。省略すると slug から作られて長くなりがちなので、短く指定するのを推奨"
      ),
  };

  server.tool(
    "senn_create_team",
    "新規チームをSENNに作成します。日本語名の場合は slug を英字で明示してください（例: app-dev）。チケット番号の頭（prefix）も指定できます。prefix が別のチームと重なると、どのチームと重なったかがエラーで返ります。",
    createTeamShape,
    async (args) => {
      try {
        const team = await client.createTeam({
          name: args.name,
          slug: args.slug,
          description: args.description,
          icon: args.icon,
          color: args.color,
          is_active: args.is_active,
          prefix: args.prefix,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(team, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_update_team ============
  const updateTeamShape = {
    team: z
      .union([z.number().int(), z.string().min(1)])
      .describe("チームの ID か slug（senn_list_teams で確認）"),
    name: z.string().min(1).optional().describe("チーム名（任意）"),
    description: z.string().optional().describe("説明（任意。空にはできない）"),
    icon: z.string().optional().describe("アイコン絵文字（任意）"),
    color: z.string().optional().describe("カラーコード（任意）"),
    prefix: z
      .string()
      .regex(/^[A-Za-z0-9]{1,20}$/)
      .optional()
      .describe(
        "チケット番号の頭（英数字1〜20文字）。変えても、作成済みのチケットの番号は変わらない（以後に作るチケットから新しい頭になる）"
      ),
    is_active: z.boolean().optional().describe("有効フラグ（任意）"),
  };

  server.tool(
    "senn_update_team",
    "既存のチームの名前・説明・アイコン・色・Prefix・有効フラグを変えます（指定した項目だけ）。チームの管理者の個人 AI キーでだけ使えます。",
    updateTeamShape,
    async (args) => {
      try {
        const team = await client.updateTeam(String(args.team), {
          name: args.name,
          description: args.description,
          icon: args.icon,
          color: args.color,
          prefix: args.prefix,
          is_active: args.is_active,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(team, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );
}
