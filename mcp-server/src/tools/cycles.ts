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

export function registerCycleTools(server: McpServer, client: WipClient) {
  // ============ senn_list_cycles ============
  const listCyclesShape = {
    project_prefix: z
      .string()
      .optional()
      .describe("プロジェクトprefix（任意。省略時は全プロジェクト対象）"),
    status: z
      .string()
      .optional()
      .describe("Cycleのステータス（任意。省略時は全ステータス対象）"),
  };

  server.tool(
    "senn_list_cycles",
    "Cycle一覧を取得します。project_prefixやstatusを指定して絞り込みできます。両方省略した場合は全Cycle一覧が返されます。",
    listCyclesShape,
    async (args) => {
      try {
        const cycles = await client.listCycles({
          projectPrefix: args.project_prefix,
          status: args.status,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(cycles, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_create_cycle ============
  const createCycleShape = {
    project_prefix: z
      .string()
      .min(1)
      .describe("プロジェクトprefix（例: SENN）"),
    name: z.string().min(1).describe("Cycleの名前"),
    description: z
      .string()
      .optional()
      .describe("Cycleの説明（任意）"),
    start_date: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .describe("開始日（YYYY-MM-DD形式）"),
    end_date: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .describe("終了日（YYYY-MM-DD形式）。start_dateより後である必要があります。"),
    status: z
      .string()
      .optional()
      .describe("ステータス（既定はplanned）"),
  };

  server.tool(
    "senn_create_cycle",
    "新規Cycleを作成します。project_prefix、name、start_date、end_dateは必須。start_date < end_dateの制約があります（違反時はWIP側が400エラーを返します）。",
    createCycleShape,
    async (args) => {
      try {
        const cycle = await client.createCycle({
          project_prefix: args.project_prefix,
          name: args.name,
          description: args.description,
          start_date: args.start_date,
          end_date: args.end_date,
          status: args.status,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(cycle, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_get_cycle ============
  const getCycleShape = {
    cycle_id: z
      .number()
      .int()
      .describe("CycleのID（senn_list_cyclesで取得）"),
  };

  server.tool(
    "senn_get_cycle",
    "Cycle詳細情報を取得します。",
    getCycleShape,
    async (args) => {
      try {
        const cycle = await client.getCycle(args.cycle_id);
        return {
          content: [{ type: "text", text: JSON.stringify(cycle, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_update_cycle ============
  const cyclePatchShape = {
    cycle_id: z
      .number()
      .int()
      .describe("CycleのID（senn_list_cyclesで取得）"),
    name: z
      .string()
      .min(1)
      .optional()
      .describe("Cycleの名前（任意）"),
    description: z
      .string()
      .optional()
      .describe("Cycleの説明（任意）"),
    start_date: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .optional()
      .describe("開始日（YYYY-MM-DD形式、任意）"),
    end_date: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .optional()
      .describe("終了日（YYYY-MM-DD形式、任意）"),
    status: z
      .string()
      .optional()
      .describe("ステータス（任意）"),
  };

  server.tool(
    "senn_update_cycle",
    "Cycleを更新します。指定したフィールドのみが変更され、省略したフィールドは既存値が保持されます。",
    cyclePatchShape,
    async (args) => {
      const { cycle_id, ...rest } = args;
      const patchableFields = ["name", "description", "start_date", "end_date", "status"] as const;

      const body: Record<string, unknown> = {};
      for (const field of patchableFields) {
        if (rest[field] !== undefined) {
          body[field] = rest[field];
        }
      }

      try {
        const updated = await client.patchCycle(cycle_id, body);
        return {
          content: [{ type: "text", text: JSON.stringify(updated, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_delete_cycle ============
  const deleteCycleShape = {
    cycle_id: z
      .number()
      .int()
      .describe("CycleのID（senn_list_cyclesで取得）"),
  };

  server.tool(
    "senn_delete_cycle",
    "Cycleを削除します。物理削除です。所属チケットのcycle フィールドはNULL化されます。",
    deleteCycleShape,
    async (args) => {
      try {
        const result = await client.deleteCycle(args.cycle_id);
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );
}
