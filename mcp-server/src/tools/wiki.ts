import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { readFile } from "fs/promises";
import { checkUploadPath } from "../upload-guard.js";
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

export function registerWikiTools(server: McpServer, client: WipClient) {
  // ============ 読み書き(ページ本体) ============
  // 省略すると全体 Wiki（どのプロジェクト・チームにも属さないページ）が対象になる
  const prefixShape = z
    .string()
    .min(1)
    .optional()
    .describe("プロジェクトprefix（例: SENN）。省略すると全体 Wiki（どのプロジェクトにも属さないページ）");
  const slugShape = z
    .string()
    .min(1)
    .describe("WikiページのslugまたはURL末尾の識別子（senn_search_wiki_pages の結果の slug）");
  const ok = (data: unknown) => ({
    content: [
      { type: "text" as const, text: data == null ? "OK" : JSON.stringify(data, null, 2) },
    ],
  });

  server.tool(
    "senn_search_wiki_pages",
    "Wikiページを検索・一覧します（本文は含まない軽量版。1ページ50件）。search はタイトル・本文の部分一致。本文は senn_get_wiki_page で取得します。project_prefix を省略すると全体 Wiki だけを対象にします。",
    {
      project_prefix: prefixShape,
      search: z.string().optional().describe("検索語（タイトル・本文の部分一致）"),
      category: z
        .string()
        .optional()
        .describe("カテゴリ（manual|minutes|spec|knowhow|glossary|other）"),
      page: z.number().int().min(1).optional().describe("ページ番号（既定1）"),
    },
    async (args) => {
      try {
        return ok(
          await client.searchWikiPages(args.project_prefix, {
            search: args.search,
            category: args.category,
            page: args.page,
          })
        );
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  server.tool(
    "senn_get_wiki_page",
    "Wikiページを1件取得します。content がMarkdown原文です。",
    { project_prefix: prefixShape, slug: slugShape },
    async (args) => {
      try {
        return ok(await client.getWikiPage(args.slug, args.project_prefix));
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  server.tool(
    "senn_list_wiki_revisions",
    "Wikiページの変更履歴（全文スナップショット・編集者・コメント）を取得します。",
    { project_prefix: prefixShape, slug: slugShape },
    async (args) => {
      try {
        return ok(await client.listWikiRevisions(args.slug, args.project_prefix));
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  server.tool(
    "senn_create_wiki_page",
    "Wikiページを作成します。本文はMarkdown。プロジェクトのメンバー権限が必要で、人ごとのAI用APIキーでのみ書き込めます。slugはタイトルから自動生成されます。project_prefix を省略すると全体 Wiki（どのプロジェクトにも属さないページ）に作成します。",
    {
      project_prefix: prefixShape,
      title: z.string().min(1).describe("ページタイトル"),
      category: z
        .string()
        .optional()
        .describe("カテゴリ（manual|minutes|spec|knowhow|glossary|other。既定other）"),
      content: z.string().optional().describe("本文（Markdown）"),
    },
    async (args) => {
      try {
        return ok(await client.createWikiPage(args));
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  server.tool(
    "senn_update_wiki_page",
    "Wikiページを更新します（指定した項目だけ変更）。content を渡すと本文が丸ごと置き換わるので、先に senn_get_wiki_page で現在の本文を取得してから編集してください。更新ごとに履歴が残ります。人ごとのAI用APIキーと書き込み権限が必要です。",
    {
      project_prefix: prefixShape,
      slug: slugShape,
      title: z.string().min(1).optional().describe("新しいタイトル（slugは変わりません）"),
      category: z.string().optional().describe("新しいカテゴリ"),
      content: z.string().optional().describe("新しい本文（Markdown全文）"),
      revision_comment: z.string().optional().describe("変更履歴に残すコメント"),
    },
    async (args) => {
      try {
        const { project_prefix, slug, ...body } = args;
        return ok(await client.updateWikiPage(slug, project_prefix, body));
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  server.tool(
    "senn_delete_wiki_page",
    "Wikiページを削除します（履歴・添付も消え、元に戻せません）。必ずユーザーの明示的な指示があるときだけ使ってください。人ごとのAI用APIキーと書き込み権限が必要です。",
    { project_prefix: prefixShape, slug: slugShape },
    async (args) => {
      try {
        return ok(await client.deleteWikiPage(args.slug, args.project_prefix));
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_list_wiki_pages ============
  const listWikiPagesShape = {
    project_prefix: z
      .string()
      .min(1)
      .describe("プロジェクトprefix（例: SENN）"),
  };

  server.tool(
    "senn_list_wiki_pages",
    "Wikiページ一覧を取得します。プロジェクト内の全Wikiページがリストアップされます。",
    listWikiPagesShape,
    async (args) => {
      try {
        const pages = await client.listWikiPages(args.project_prefix);
        return {
          content: [{ type: "text", text: JSON.stringify(pages, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_list_wiki_attachments ============
  const listWikiAttachmentsShape = {
    project_prefix: z
      .string()
      .min(1)
      .describe("プロジェクトprefix（例: SENN）"),
    slug: z.string().min(1).describe("WikiページのID/スラッグ"),
  };

  server.tool(
    "senn_list_wiki_attachments",
    "Wikiページの添付ファイル一覧を取得します。ファイル名・ID・アップロード日時などが返されます。",
    listWikiAttachmentsShape,
    async (args) => {
      try {
        const attachments = await client.listWikiAttachments(
          args.slug,
          args.project_prefix
        );
        return {
          content: [
            { type: "text", text: JSON.stringify(attachments, null, 2) },
          ],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_upload_wiki_attachment ============
  /**
   * Wikiページに添付ファイルをアップロードする。
   *
   * ※本ツールはstdio接続（ローカル）でのみ動作します。
   * file_pathはこのMCPサーバープロセスから見えるローカル絶対パスである必要があるため、
   * 将来リモート接続（SSE/HTTP transport等）に対応する場合は、ファイル本体を
   * base64等で受け渡す別方式への変更が必要になる。
   */
  const uploadWikiAttachmentShape = {
    project_prefix: z
      .string()
      .min(1)
      .describe("プロジェクトprefix（例: SENN）"),
    slug: z.string().min(1).describe("WikiページのID/スラッグ"),
    file_path: z
      .string()
      .min(1)
      .describe(
        "ローカル絶対ファイルパス（MCPサーバーが実行されているマシン上のパス）。添付できるのは、作業中のフォルダ（または SENN_UPLOAD_ROOTS）の下のファイルだけ。「.」で始まるフォルダ・鍵・認証情報は添付できない"
      ),
  };

  server.tool(
    "senn_upload_wiki_attachment",
    "Wikiページに添付ファイルをアップロードします。file_pathはこのMCPサーバーが実行されているマシン上のローカル絶対パスを指定してください（Base64ではなく、ファイルパス文字列を渡します）。",
    uploadWikiAttachmentShape,
    async (args) => {
      try {
        // 送ってよいファイルかを確かめてから読む(秘密のファイルを持ち出させない)
        const checked = await checkUploadPath(args.file_path);
        if (!checked.ok) {
          return { isError: true, content: [{ type: "text", text: checked.reason }] };
        }
        let fileBuffer: Buffer;
        try {
          fileBuffer = await readFile(checked.path);
        } catch (err) {
          return {
            isError: true,
            content: [
              {
                type: "text",
                text: `ファイル読み込みエラー: ${args.file_path}（存在しない、または読み取り権限がありません）`,
              },
            ],
          };
        }

        // FormDataに追加
        const form = new FormData();
        // BufferをUint8Arrayに変換してBlobに渡す
        const blob = new Blob([new Uint8Array(fileBuffer)]);
        const filename = checked.filename;
        form.append("file", blob, filename);

        const result = await client.uploadWikiAttachment(
          args.slug,
          args.project_prefix,
          form
        );
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_replace_wiki_attachment ============
  /**
   * Wikiページの添付ファイルを置換する。
   *
   * ※本ツールはstdio接続（ローカル）でのみ動作します。
   * file_pathはこのMCPサーバープロセスから見えるローカル絶対パスである必要があるため、
   * 将来リモート接続（SSE/HTTP transport等）に対応する場合は、ファイル本体を
   * base64等で受け渡す別方式への変更が必要になる。
   */
  const replaceWikiAttachmentShape = {
    slug: z.string().min(1).describe("WikiページのID/スラッグ"),
    attachment_id: z
      .number()
      .int()
      .describe("置換対象の添付ファイルID（senn_list_wiki_attachmentsで取得）"),
    file_path: z
      .string()
      .min(1)
      .describe(
        "ローカル絶対ファイルパス（MCPサーバーが実行されているマシン上のパス）。添付できるのは、作業中のフォルダ（または SENN_UPLOAD_ROOTS）の下のファイルだけ。「.」で始まるフォルダ・鍵・認証情報は添付できない"
      ),
  };

  server.tool(
    "senn_replace_wiki_attachment",
    "Wikiページの添付ファイルを置換します。既存のファイルを新しいファイルで上書きします。file_pathはこのMCPサーバーが実行されているマシン上のローカル絶対パスを指定してください。",
    replaceWikiAttachmentShape,
    async (args) => {
      try {
        // 送ってよいファイルかを確かめてから読む(秘密のファイルを持ち出させない)
        const checked = await checkUploadPath(args.file_path);
        if (!checked.ok) {
          return { isError: true, content: [{ type: "text", text: checked.reason }] };
        }
        let fileBuffer: Buffer;
        try {
          fileBuffer = await readFile(checked.path);
        } catch (err) {
          return {
            isError: true,
            content: [
              {
                type: "text",
                text: `ファイル読み込みエラー: ${args.file_path}（存在しない、または読み取り権限がありません）`,
              },
            ],
          };
        }

        // FormDataに追加
        const form = new FormData();
        // BufferをUint8Arrayに変換してBlobに渡す
        const blob = new Blob([new Uint8Array(fileBuffer)]);
        const filename = checked.filename;
        form.append("file", blob, filename);

        const result = await client.replaceWikiAttachment(
          args.slug,
          args.attachment_id,
          form
        );
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      } catch (e) {
        return toErrorResult(e);
      }
    }
  );

  // ============ senn_delete_wiki_attachment ============
  const deleteWikiAttachmentShape = {
    slug: z.string().min(1).describe("WikiページのID/スラッグ"),
    attachment_id: z
      .number()
      .int()
      .describe("削除対象の添付ファイルID（senn_list_wiki_attachmentsで取得）"),
  };

  server.tool(
    "senn_delete_wiki_attachment",
    "Wikiページの添付ファイルを削除します。",
    deleteWikiAttachmentShape,
    async (args) => {
      try {
        const result = await client.deleteWikiAttachment(
          args.slug,
          args.attachment_id
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
