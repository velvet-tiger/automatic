/** The parts of a Discover catalogue entry needed to build a saved MCP config. */
export interface DiscoverMcpSource {
  provider: string;
  repository_url: string | null;
  remote: { transport: string; url: string } | null;
  local: { command: string; args?: string[] | null } | null;
  auth: { env_vars: Array<{ name: string }> };
}

/** Resolve a Discover `local` block into a runnable {command, args}.
 *  Prefers an explicit `args` array; falls back to splitting the command
 *  string on whitespace for legacy entries that embed args in `command`.
 *  Splitting a string is fragile (it breaks on arguments containing spaces),
 *  so `args` is the canonical source when an entry provides it. */
export function resolveLocalCommand(
  local: NonNullable<DiscoverMcpSource["local"]>,
): { command: string; args: string[] } {
  const parts = local.command.split(/\s+/).filter(Boolean);
  return { command: parts[0] || "", args: local.args ?? parts.slice(1) };
}

/** Build a save-ready config from Discover data.
 *
 *  Prefers the remote endpoint when an entry offers one: a hosted server needs
 *  no local runtime (Docker, Node) and cannot leave processes behind. Entries
 *  with only a local block build a stdio config.
 *
 *  `auth.env_vars` describe what the local process reads. They are only added
 *  to a remote config when the entry has no local block, which keeps existing
 *  remote-only entries unchanged.
 *
 *  Embeds `_author` metadata so Automatic can display the provider in the MCP
 *  Servers view.
 *
 *  Returns `null` for an entry with neither block: it needs manual setup, and
 *  there is no config Automatic could save that would run. */
export function buildDiscoverMcpConfig(server: DiscoverMcpSource): Record<string, unknown> | null {
  const _author: Record<string, string> = { name: server.provider };
  if (server.repository_url) _author.repository_url = server.repository_url;

  const env: Record<string, string> = {};
  server.auth.env_vars.forEach((v) => {
    env[v.name] = "";
  });

  if (server.remote) {
    const type = server.remote.transport === "sse" ? "sse" : "http";
    const cfg: Record<string, unknown> = { type, url: server.remote.url, _author };
    if (!server.local && Object.keys(env).length > 0) cfg.env = env;
    return cfg;
  }
  if (server.local) {
    const { command, args } = resolveLocalCommand(server.local);
    const cfg: Record<string, unknown> = { type: "stdio", command, _author };
    if (args.length > 0) cfg.args = args;
    if (Object.keys(env).length > 0) cfg.env = env;
    return cfg;
  }
  return null;
}
