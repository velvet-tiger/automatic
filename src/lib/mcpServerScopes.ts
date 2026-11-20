import { invoke } from "@tauri-apps/api/core";

/**
 * Library MCP servers that every project can use, sorted by name.
 *
 * Project servers are left out. They belong to one project, so profiles,
 * templates and other shared lists cannot use them, and the backend refuses
 * a save that names one.
 */
export async function listSharedMcpServers(): Promise<string[]> {
  const [names, scopes] = await Promise.all([
    invoke<string[]>("list_mcp_server_configs"),
    invoke<Record<string, string>>("list_mcp_server_scopes"),
  ]);
  return names.filter((name) => scopes[name] === undefined).sort();
}
