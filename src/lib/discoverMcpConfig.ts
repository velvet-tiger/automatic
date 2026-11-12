import { fillUrlTemplate, urlTemplateTokens } from "./urlTemplate";

/** The parts of a Discover catalogue entry needed to build a saved MCP config. */
export interface DiscoverMcpSource {
  provider: string;
  repository_url: string | null;
  /** `headers` may reference an environment variable as `${NAME}`. Automatic
   *  rewrites the reference into each agent's own syntax when it syncs. */
  remote: { transport: string; url: string; headers?: Record<string, string> | null } | null;
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
 *  `remote.headers` are copied as they are. An entry uses them for a hosted
 *  endpoint that authenticates with a token header and not with OAuth.
 *
 *  Embeds `_author` metadata so Automatic can display the provider in the MCP
 *  Servers view.
 *
 *  A remote URL that holds `<token>` placeholders (see `urlTemplate.ts`) is
 *  filled from `urlValues`, and the template is kept as `_url_template` so the
 *  editor can ask for the values again. Without `urlValues` the URL keeps its
 *  placeholders. That form is for display only: `needsUrlValues` reports it,
 *  and a caller must not save it.
 *
 *  Returns `null` for an entry with neither block: it needs manual setup, and
 *  there is no config Automatic could save that would run. Throws when
 *  `urlValues` is given but cannot fill the template.
 */
export function buildDiscoverMcpConfig(
  server: DiscoverMcpSource,
  urlValues?: Record<string, string>,
): Record<string, unknown> | null {
  const _author: Record<string, string> = { name: server.provider };
  if (server.repository_url) _author.repository_url = server.repository_url;

  const env: Record<string, string> = {};
  server.auth.env_vars.forEach((v) => {
    env[v.name] = "";
  });

  if (server.remote) {
    const type = server.remote.transport === "sse" ? "sse" : "http";
    const cfg: Record<string, unknown> = { type, url: server.remote.url, _author };
    if (urlTemplateTokens(server.remote.url).length > 0) {
      cfg._url_template = server.remote.url;
      if (urlValues) {
        const filled = fillUrlTemplate(server.remote.url, urlValues);
        if ("problem" in filled) throw new Error(filled.problem);
        cfg.url = filled.url;
      }
    }
    const headers = server.remote.headers ?? {};
    if (Object.keys(headers).length > 0) cfg.headers = { ...headers };
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

/** The URL tokens a user must fill before `server` can be saved. Empty when none. */
export function discoverUrlTokens(server: DiscoverMcpSource): string[] {
  return server.remote ? urlTemplateTokens(server.remote.url) : [];
}

/** True when a built config still holds URL placeholders and must not be saved. */
export function needsUrlValues(config: Record<string, unknown>): boolean {
  return typeof config.url === "string" && urlTemplateTokens(config.url).length > 0;
}
