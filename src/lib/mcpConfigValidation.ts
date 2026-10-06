import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";

/** Mirrors `McpFindingSeverity` in `src-tauri/src/core/mcp_docker.rs`. */
export type McpFindingSeverity = "error" | "warning" | "info";

export interface McpConfigFinding {
  severity: McpFindingSeverity;
  code: string;
  message: string;
}

/** An official hosted endpoint that replaces a container image. */
export interface McpRemoteEquivalent {
  title: string;
  url: string;
  transport: "http" | "sse";
  /** Shape of the auth header the endpoint accepts. Display only. */
  auth_header: string;
}

export interface McpConfigValidation {
  findings: McpConfigFinding[];
  /** True when the one-click Docker fix would change the arguments. */
  can_fix: boolean;
  remote_equivalent?: McpRemoteEquivalent;
}

export function hasMcpConfigErrors(validation: McpConfigValidation | null): boolean {
  return !!validation && validation.findings.some((f) => f.severity === "error");
}

/** Validate a config exactly as it would be saved. */
export function validateMcpConfig(config: Record<string, unknown>): Promise<McpConfigValidation> {
  return invoke<McpConfigValidation>("validate_mcp_server_config", {
    data: JSON.stringify(config),
  });
}

/** Validate every stored config. Only servers with findings are returned. */
export function validateStoredMcpConfigs(): Promise<Record<string, McpConfigValidation>> {
  return invoke<Record<string, McpConfigValidation>>("validate_stored_mcp_server_configs");
}

/** Arguments with `-i` and `--rm` inserted after `run` when they are missing. */
export function fixMcpDockerArgs(command: string, args: string[]): Promise<string[]> {
  return invoke<string[]>("fix_mcp_docker_args", { command, args });
}

/**
 * Gate for entry points that save a config without showing the editor
 * (Discover, paste import, project recommendations). Returns true when the
 * config has no errors, or when the user confirms saving it anyway.
 * Backticks are dropped from messages because the native dialog shows plain text.
 */
export async function confirmMcpConfigSave(
  name: string,
  config: Record<string, unknown>,
): Promise<boolean> {
  const validation = await validateMcpConfig(config);
  const errors = validation.findings.filter((f) => f.severity === "error");
  if (errors.length === 0) return true;
  const detail = errors.map((f) => `• ${f.message.replace(/`/g, "")}`).join("\n");
  return ask(
    `"${name}" cannot start with these settings:\n\n${detail}\n\nAutomatic will not sync it to your agents until it is fixed. Add it anyway?`,
    { title: "MCP server has errors", kind: "warning" },
  );
}
