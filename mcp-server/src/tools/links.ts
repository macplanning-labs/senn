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

export function registerLinkTools(server: McpServer, client: WipClient) {
  // ============ senn_list_ticket_links ============
  const listTicketLinksShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: SENN-000069）"),
  };

  server.tool(
    "senn_list_ticket_links",
    "チケットの参照リンク一覧を取得します。外部サイト・ドキュメント・関連リソースへのリンクが一覧表示されます。",
    listTicketLinksShape,
    async (args) => {
      try {
        const links = await client.listTicketLinks(args.ticket_key);
        return {
          content: [{ type: "text", text: JSON.stringify(links, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_add_ticket_link ============
  const addTicketLinkShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: SENN-000069）"),
    url: z
      .string()
      .url()
      .describe("参照リンク先のURL"),
    title: z
      .string()
      .optional()
      .describe("リンクのタイトル・説明（任意）"),
  };

  server.tool(
    "senn_add_ticket_link",
    "チケットに参照リンクを追加します。外部サイト・ドキュメント・関連する記事などのリンクをチケットに紐付けます。",
    addTicketLinkShape,
    async (args) => {
      try {
        const link = await client.addTicketLink(args.ticket_key, {
          url: args.url,
          title: args.title,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(link, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_delete_ticket_link ============
  const deleteTicketLinkShape = {
    ticket_key: z
      .string()
      .min(1)
      .describe("チケットキー（例: SENN-000069）"),
    link_id: z
      .number()
      .int()
      .describe("リンクID（senn_list_ticket_linksで取得）"),
  };

  server.tool(
    "senn_delete_ticket_link",
    "チケットの参照リンクを削除します。リンクIDはwip_list_ticket_linksで確認できます。",
    deleteTicketLinkShape,
    async (args) => {
      try {
        const result = await client.deleteTicketLink(
          args.ticket_key,
          args.link_id
        );
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );
}
