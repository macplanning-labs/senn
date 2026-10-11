#!/usr/bin/env node

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { loadConfig } from "./config.js";
import { WipClient } from "./client.js";
import { registerAllTools } from "./tools/index.js";
import { PairingWatcher } from "./pairing-watcher.js";

async function main() {
  // 環境変数の読み込み・検証
  const config = loadConfig();

  // MCPサーバーの生成
  const server = new McpServer({
    name: "senn-mcp-server",
    version: "0.1.0",
  });

  // WipClientの生成
  const client = new WipClient(config.baseUrl, config.headers);

  // 端末の接続(承認を待つ間の見張り。待ちの状態が残っていれば再開する)
  const watcher = new PairingWatcher(client);
  watcher.resume();

  // ツール登録
  registerAllTools(server, client, watcher);

  // StdioServerTransportに接続
  const transport = new StdioServerTransport();
  await server.connect(transport);

  console.error("SENN MCP Server started");
}

main().catch((err) => {
  console.error("Fatal error:", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
