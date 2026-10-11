import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { WipClient } from "../client.js";
import { registerProjectTools } from "./projects.js";
import { registerTicketTools } from "./tickets.js";
import { registerDependencyTools } from "./dependencies.js";
import { registerLinkTools } from "./links.js";
import { registerCycleTools } from "./cycles.js";
import { registerTeamTools } from "./teams.js";
import { registerWikiTools } from "./wiki.js";
import { registerRunTools } from "./runs.js";
import { registerConnectTools } from "./connect.js";
import type { PairingWatcher } from "../pairing-watcher.js";

export function registerAllTools(server: McpServer, client: WipClient, watcher: PairingWatcher) {
  registerProjectTools(server, client);
  registerTicketTools(server, client);
  registerDependencyTools(server, client);
  registerLinkTools(server, client);
  registerCycleTools(server, client);
  registerTeamTools(server, client);
  registerWikiTools(server, client);
  registerRunTools(server, client);
  registerConnectTools(server, client, watcher);
}
