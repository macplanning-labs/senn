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

export function registerTicketTools(server: McpServer, client: WipClient) {
  // ============ senn_list_tickets ============
  const listTicketsShape = {
    project_prefix: z
      .string()
      .min(1)
      .describe("プロジェクトprefix（例: SENN, AIDA）"),
    page: z
      .number()
      .int()
      .min(1)
      .optional()
      .describe("ページ番号(1始まり)。1ページ50件、更新日時降順。省略時は1。"),
    status: z
      .array(z.string().min(1))
      .optional()
      .describe("この状態のものだけ（複数可。例: [\"open\", \"in_progress\"]）"),
    status_not: z
      .array(z.string().min(1))
      .optional()
      .describe(
        "この状態のものを除く（複数可）。未完了だけなら [\"resolved\", \"closed\", \"canceled\"]"
      ),
    label: z.string().min(1).optional().describe("このラベル名が付いたものだけ"),
    cycle: z.number().int().optional().describe("この Cycle の ID のものだけ（senn_list_cycles で確認）"),
    assignee: z.string().min(1).optional().describe("この担当者（ユーザー名）のものだけ"),
    updated_after: z
      .string()
      .min(1)
      .optional()
      .describe(
        "この日時以降に更新されたものだけ。日時（例: 2026-10-09T00:00:00+09:00）か日付（例: 2026-10-09。その日の0時UTC）"
      ),
  };

  server.tool(
    "senn_list_tickets",
    "指定プロジェクト内のチケット一覧を取得します。状態・除く状態・ラベル名・Cycle・担当者・更新日時で絞り込めます（例:「この Cycle の未完了」= cycle と status_not=[resolved, closed, canceled]）。1ページ50件、更新日時降順。51件以上ある場合は`page`を2,3...と増やして呼び出すことで全件取得できます。",
    listTicketsShape,
    async (args) => {
      try {
        const tickets = await client.listTickets(args.project_prefix, args.page, {
          status: args.status,
          status_not: args.status_not,
          label: args.label,
          cycle: args.cycle,
          assignee: args.assignee,
          updated_after: args.updated_after,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(tickets, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_get_ticket ============
  const getTicketShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: SENN-000069）"),
  };

  server.tool(
    "senn_get_ticket",
    "チケット詳細情報を取得します。ステータス・優先度・アサイン・スプリント情報など、チケットの全情報を返します。",
    getTicketShape,
    async (args) => {
      try {
        const ticket = await client.getTicket(args.ticket_key);
        return {
          content: [{ type: "text", text: JSON.stringify(ticket, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_create_ticket ============
  const createTicketShape = {
    project_prefix: z
      .string()
      .min(1)
      .optional()
      .describe("プロジェクトprefix（例: SENN）。team_id/team_slugとの併用は不可。project_prefixもteam_id/team_slugも無い場合は400"),
    title: z.string().min(1).describe("チケットタイトル"),
    description: z.string().optional().describe("説明（Markdown対応）"),
    priority: z
      .string()
      .optional()
      .describe(
        "優先度（low|medium|high|urgent）。既定はmedium。"
      ),
    ticket_type: z
      .string()
      .optional()
      .describe("チケット種別（task|bug|feature等）。既定はtask。"),
    due_date: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .optional()
      .describe("期日（YYYY-MM-DD形式）"),
    labels: z
      .array(z.string())
      .optional()
      .describe("ラベル名の配列。存在しないラベルは自動作成されます。"),
    assignee_usernames: z
      .array(z.string())
      .optional()
      .describe(
        "アサイン先ユーザー名の配列。プロジェクト未参加なら自動メンバー追加されます。"
      ),
    team_id: z
      .number()
      .int()
      .optional()
      .describe("チームID。project_prefixの代わりに指定可。team_slugとの併用は不可。プロジェクト無しでもチーム指定で作成できます。"),
    team_slug: z
      .string()
      .min(1)
      .optional()
      .describe("チームのslug。project_prefixの代わりに指定可。team_idとの併用は不可。プロジェクト無しでもチーム指定で作成できます。"),
    status: z
      .string()
      .min(1)
      .optional()
      .describe(
        "状態のslug（既定 open）。そのプロジェクト・チームに登録された状態だけ（例: backlog, open, in_progress, resolved）。done など登録の無い値はエラーで、使える状態の一覧が返ります。着手の状態で作ると開始日が入ります"
      ),
    story_points: z
      .number()
      .int()
      .optional()
      .describe("ストーリーポイント（1, 2, 3, 5, 8, 13, 21 のどれか）"),
    cycle: z
      .number()
      .int()
      .optional()
      .describe("Cycle の ID（そのプロジェクト・チームの Cycle。senn_list_cycles で確認）"),
    parent_key: z
      .string()
      .min(1)
      .optional()
      .describe("親チケットのキー（例: SENN-000001）"),
    start_date: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .optional()
      .describe("開始日（YYYY-MM-DD形式）"),
  };

  server.tool(
    "senn_create_ticket",
    "新規チケットを作成します。titleは必須。project_prefix、team_id、team_slugのいずれか1つが必須です。状態・ストーリーポイント・Cycle・親・開始日も1回で指定できます（作成後に senn_update_ticket を呼ぶ必要はありません）。複数のチケットを依存関係ごと作るときは senn_bulk_create_tickets を使ってください。",
    createTicketShape,
    async (args) => {
      try {
        const ticket = await client.createTicket({
          project_prefix: args.project_prefix,
          title: args.title,
          description: args.description,
          priority: args.priority,
          ticket_type: args.ticket_type,
          due_date: args.due_date,
          labels: args.labels,
          assignee_usernames: args.assignee_usernames,
          team_id: args.team_id,
          team_slug: args.team_slug,
          status: args.status,
          story_points: args.story_points,
          cycle: args.cycle,
          parent_key: args.parent_key,
          start_date: args.start_date,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(ticket, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_update_ticket ============
  // 設計書§7.2「senn_update_ticketのZodスキーマ」をそのまま使用
  const ticketPatchShape = {
    ticket_key: z.string().describe("チケットキー(例: SENN-000001)"),

    // 単純系: 触らない場合は省略するだけでよい
    title: z.string().min(1).optional(),
    description: z.string().optional(),
    status: z.string().optional(),
    priority: z.string().optional(),
    ticket_type: z.string().optional(),

    // null許容系: 省略=触らない / null=クリア を区別するため .nullable().optional()
    category: z.number().int().nullable().optional(),
    milestone: z.number().int().nullable().optional(),
    parent: z.number().int().nullable().optional(),
    cycle: z.number().int().nullable().optional(),
    story_points: z.number().int().nullable().optional(),
    start_date: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .nullable()
      .optional(),
    due_date: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .nullable()
      .optional(),

    // 配列系: 省略=触らない / 空配列[]=全解除
    assignees: z.array(z.number().int()).optional(),
    labels: z
      .array(z.number().int())
      .optional()
      .describe("ラベルの ID で置き換える（ふつうは下の名前での指定を使う）"),
    label_names: z
      .array(z.string().min(1))
      .optional()
      .describe("ラベルを、この名前のものに置き換える（無いラベルは作る。[] で全部外す）"),
    add_labels: z
      .array(z.string().min(1))
      .optional()
      .describe("このラベル名を足す（無いラベルは作る）"),
    remove_labels: z
      .array(z.string().min(1))
      .optional()
      .describe("このラベル名を外す（付いていなければ何もしない）"),
    linked_rules: z.array(z.number().int()).optional(),
  };

  server.tool(
    "senn_update_ticket",
    "チケットを更新します。指定したフィールドのみが変更され、省略したフィールドは変更されません。ステータス変更・優先度変更・アサイン変更など、部分的な更新に使います。ラベルは名前で足す（add_labels）・外す（remove_labels）・置き換える（label_names）ことができます。status は、そのプロジェクト・チームに登録された状態だけ（done などはエラー）。チケットを完全に不要と判断した場合は senn_delete_ticket を使ってください。",
    ticketPatchShape,
    async (args) => {
      const { ticket_key, ...rest } = args;
      const patchableFields = [
        "title",
        "description",
        "status",
        "priority",
        "ticket_type",
        "category",
        "milestone",
        "parent",
        "cycle",
        "story_points",
        "start_date",
        "due_date",
        "assignees",
        "labels",
        "label_names",
        "add_labels",
        "remove_labels",
        "linked_rules",
      ] as const;

      const body: Record<string, unknown> = {};
      for (const field of patchableFields) {
        // undefinedのキーだけ除外(nullはそのまま送る)
        if (rest[field] !== undefined) {
          body[field] = rest[field];
        }
      }

      try {
        const updated = await client.patchTicket(ticket_key, body);
        return { content: [{ type: "text", text: JSON.stringify(updated, null, 2) }] };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_bulk_create_tickets ============
  const bulkTicketShape = z.object({
    ref: z
      .string()
      .min(1)
      .describe("この一括の中での仮の名前（例: T1）。parent・depends_on で使う"),
    title: z.string().min(1),
    description: z.string().optional(),
    priority: z.string().optional().describe("low|medium|high|urgent（既定 medium）"),
    ticket_type: z.string().optional().describe("task|bug|feature 等（既定 task）"),
    status: z.string().min(1).optional().describe("状態のslug（既定 open）"),
    story_points: z.number().int().optional().describe("1, 2, 3, 5, 8, 13, 21"),
    start_date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional(),
    due_date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional(),
    cycle: z.number().int().optional().describe("Cycle の ID"),
    labels: z.array(z.string()).optional().describe("ラベル名（無いものは作る）"),
    assignee_usernames: z.array(z.string()).optional(),
    parent: z
      .string()
      .min(1)
      .optional()
      .describe("親。この一括の ref か、既存のチケットのキー"),
    depends_on: z
      .array(z.string().min(1))
      .optional()
      .describe(
        "先に終わらせるもの（この一括の ref か、既存のチケットのキー）。それが終わらないと、このチケットに進めない（blocks で登録）"
      ),
  });
  const bulkCreateShape = {
    project_prefix: z.string().min(1).optional().describe("プロジェクトprefix"),
    team_id: z.number().int().optional().describe("チームID（project_prefix の代わり）"),
    team_slug: z.string().min(1).optional().describe("チームのslug（project_prefix の代わり）"),
    dry_run: z
      .boolean()
      .optional()
      .describe("true なら、確かめるだけで何も登録しない（ticket_key は仮の番号）"),
    tickets: z.array(bulkTicketShape).min(1).max(100).describe("登録するチケット（1〜100件）"),
  };

  server.tool(
    "senn_bulk_create_tickets",
    "複数のチケットと、その間の依存関係・親子を、1回でまとめて登録します（実装計画からの一括起票向け）。計画の中の仮の名前（ref）で parent・depends_on を書くと、登録時に本当の番号に置き換えます。全部登録できるか、何も登録しないかのどちらかです（循環する依存・登録の無い状態・見つからない参照などが1件でもあれば、何も登録せずエラー）。先に dry_run=true で確かめるのを推奨。結果は ref と ticket_key の対応と、登録した依存（from=先 → to=後）です。",
    bulkCreateShape,
    async (args) => {
      try {
        const result = await client.bulkCreateTickets(args as Record<string, unknown>);
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_delete_ticket ============
  const deleteTicketShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: SENN-000069）"),
    reason: z
      .string()
      .min(1)
      .describe(
        "削除理由（監査コメントとしてSENNに記録されます）。必須です。"
      ),
  };

  server.tool(
    "senn_delete_ticket",
    "チケットを削除します（論理削除）。ステータスがcanceledに変わり、監査コメントが自動投稿されます。単なるステータス変更なら senn_update_ticket でstatus=canceledを指定してください。チケットが完全に不要と判断した場合のみこのツールを使ってください。",
    deleteTicketShape,
    async (args) => {
      try {
        const result = await client.deleteTicket(
          args.ticket_key,
          args.reason
        );
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_add_ticket_comment ============
  const addTicketCommentShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: SENN-000069）"),
    body: z.string().min(1).describe("コメント本文（Markdown対応）"),
  };

  server.tool(
    "senn_add_ticket_comment",
    "チケットにコメントを追加します。Markdown形式でフォーマット可能です。プロジェクトメンバーにメンションするには本文に `@表示名` または `@ユーザー名` を書きます（例: @山田太郎）。メンションされた人に通知が飛びます。",
    addTicketCommentShape,
    async (args) => {
      try {
        const comment = await client.addTicketComment(args.ticket_key, {
          body: args.body,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(comment, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );
  // ============ senn_update_ticket_comment ============
  server.tool(
    "senn_update_ticket_comment",
    "既存のコメントの本文を書き換えます（全文置き換え）。直せるのは、自分がAI経由で書いたコメントと、共有AIアカウントが書いた実行者不明のコメントだけです。人が画面で書いたコメントや、ほかの人がAI経由で書いたコメントは直せません。編集で新しく増えたメンションだけ通知されます（@表示名 または @ユーザー名）。先にチケット取得でコメントIDと現在の本文を確認してください。",
    {
      ticket_key: z.string().min(1).describe("チケットキー（例: SENN-000069）"),
      comment_id: z.number().int().min(1).describe("編集するコメントのID"),
      body: z.string().min(1).describe("新しいコメント本文（Markdown全文）"),
    },
    async (args) => {
      try {
        const comment = await client.updateTicketComment(args.ticket_key, args.comment_id, {
          body: args.body,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(comment, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );
}
