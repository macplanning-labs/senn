import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { WipClient, WipApiError } from "../client.js";
import {
  defaultDir,
  defaultLabel,
  generateKey,
  hashKey,
  keyPrefix12,
  saveState,
} from "../pairing.js";
import type { PairingWatcher } from "../pairing-watcher.js";

function text(t: string) {
  return { content: [{ type: "text" as const, text: t }] };
}

/**
 * 端末の接続(コピー・貼り付けなし)。キーはこの Mac の中で作り、画面にも通信にも出さない。
 * 申請・承認の流れは、利用者が「許可を求める」を1回押し、管理者が承認するだけ。
 */
export function registerConnectTools(server: McpServer, client: WipClient, watcher: PairingWatcher) {
  server.tool(
    "senn_connect_account",
    "この Mac で AI Agent(SENN の MCP)を使えるように、利用者本人の接続を申請します。返ったリンクを利用者に伝え、「許可を求める」を押してもらってください。管理者が承認すると、自動で使えるようになります(キーのコピーや貼り付けは要りません)。キーや秘密の値は、返答に含まれません。",
    {
      label: z
        .string()
        .max(100)
        .optional()
        .describe("この Mac の名前(省略すると、ホスト名)。承認する管理者に表示されます"),
    },
    async (args) => {
      try {
        const existing = watcher.current();
        if (existing) {
          watcher.watch();
          return text(
            `すでに接続の手続き中です。次のリンクを開いて、「許可を求める」を押してください(まだなら)。\n${existing.url}\n承認されると、自動で使えるようになります。`
          );
        }
        const key = generateKey();
        const label = (args.label ?? defaultLabel()) || "Mac";
        const started = await client.startPairing({
          keyHash: hashKey(key),
          keyPrefix: keyPrefix12(key),
          label,
        });
        saveState(defaultDir(), {
          code: started.code,
          deviceSecret: started.deviceSecret,
          key,
          url: started.url,
          label,
          createdAt: new Date().toISOString(),
        });
        watcher.watch();
        return text(
          `次のリンクを開いて、「許可を求める」を押してください(${Math.round(started.expiresInSeconds / 60)}分以内)。\n${started.url}\n管理者が承認すると、自動で使えるようになります。キーの操作は要りません。`
        );
      } catch (e) {
        if (e instanceof WipApiError) {
          return { isError: true, content: [{ type: "text" as const, text: `SENNエラー(HTTP ${e.status}): ${e.message}` }] };
        }
        throw e;
      }
    }
  );

  server.tool(
    "senn_connect_status",
    "AI Agent の接続の状態(未申請・申請中・接続済み・終了)を確認します。値は含まれません。",
    {},
    async () => {
      const st = watcher.current();
      if (st) {
        const outcome = await watcher.checkOnce();
        if (outcome?.kind === "connected") return text("接続しました。この Mac で、そのまま使えます。");
        if (outcome?.kind === "closed") return text(closedMessage(outcome.status, outcome.message));
        const s = watcher.lastStatus ?? "waiting";
        return text(
          s === "requested"
            ? "許可を求めました。管理者の承認を待っています(承認されると、自動で使えるようになります)。"
            : `まだ、リンクで「許可を求める」が押されていません。\n${st.url}`
        );
      }
      const r = watcher.lastResult;
      if (r.kind === "connected") return text("接続しました。この Mac で、そのまま使えます。");
      if (r.kind === "closed") return text(closedMessage(r.status, r.message));
      return text("接続の手続きは、していません(または、すでに完了しています)。必要なら senn_connect_account を実行してください。");
    }
  );
}

function closedMessage(status: "rejected" | "failed" | "expired", message: string | null): string {
  const base =
    status === "rejected"
      ? "許可されませんでした。"
      : status === "failed"
        ? "接続できませんでした。"
        : "リンクが期限切れでした。もう一度 senn_connect_account を実行してください。";
  return message ? `${base}(${message})` : base;
}
