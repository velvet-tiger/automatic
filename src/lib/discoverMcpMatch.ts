import featuredMcpServers from "../../src-tauri/assets/discover/featured-mcp-servers.json";
import { resolveLocalCommand, type DiscoverMcpSource } from "./discoverMcpConfig";
import { fillUrlTemplate, readUrlTemplateValues } from "./urlTemplate";

/** The Discover catalogue fields the MCP Servers page shows for a matched server. */
export interface DiscoverCatalogueEntry extends DiscoverMcpSource {
  slug: string;
  title: string;
  description: string;
  /** Brand domain for the Brandfetch logo, e.g. "aikido.dev". */
  icon?: string | null;
  classification: string;
  docs_url?: string | null;
  auth: {
    method?: string;
    env_vars: Array<{ name: string; token_url?: string | null; token_url_label?: string | null }>;
  };
}

/** The parts of a saved server config that identify what it connects to. */
export interface SavedMcpConnection {
  type?: string;
  command?: string;
  args?: string[];
  url?: string;
  _url_template?: string;
}

export const DISCOVER_CATALOGUE = featuredMcpServers as unknown as DiscoverCatalogueEntry[];

function sameArgs(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((value, i) => value === b[i]);
}

/** Whether `config` connects to what `entry` describes. */
function connectsLike(config: SavedMcpConnection, entry: DiscoverCatalogueEntry): boolean {
  if (config.type === "stdio" || (!config.type && config.command)) {
    if (!entry.local || !config.command) return false;
    const { command, args } = resolveLocalCommand(entry.local);
    return config.command === command && sameArgs(config.args ?? [], args);
  }
  if (!entry.remote) return false;
  const catalogueUrl = entry.remote.url;
  if (config.url === catalogueUrl) return true;
  if (!config.url || config._url_template !== catalogueUrl) return false;
  return fillsTemplate(catalogueUrl, config.url);
}

/**
 * Whether `url` is a valid fill of `template`: each placeholder holds one
 * safe host label and every other part is the template's own. Without this,
 * a config could copy a catalogue template into `_url_template` and point
 * `url` at any host, and still be shown as the catalogue server.
 */
function fillsTemplate(template: string, url: string): boolean {
  const filled = fillUrlTemplate(template, readUrlTemplateValues(template, url));
  return "url" in filled && filled.url === url;
}

/**
 * The Discover catalogue entry a saved server was installed from.
 *
 * Matches on what the server connects to (command and arguments, or URL),
 * never on its name or `_author`. A name can be anything, and `_author` can
 * come from a third-party source manifest, so either alone would let any
 * server show a catalogue logo and link. A renamed copy still matches.
 * When several entries match, the one whose slug is the server's name wins.
 */
export function matchDiscoverEntry(
  serverName: string,
  config: SavedMcpConnection,
  catalogue: DiscoverCatalogueEntry[] = DISCOVER_CATALOGUE,
): DiscoverCatalogueEntry | null {
  const matches = catalogue.filter((entry) => connectsLike(config, entry));
  if (matches.length === 0) return null;
  const needle = serverName.toLowerCase();
  return matches.find((entry) => entry.slug === needle) ?? matches[0] ?? null;
}

/** Token-acquisition links for the env vars a matched server's config holds. */
export function tokenLinksFor(
  entry: DiscoverCatalogueEntry | null,
  env: Record<string, string>,
): Array<{ name: string; url: string; label: string }> {
  const vars = entry?.auth.env_vars;
  if (!vars) return [];
  const out: Array<{ name: string; url: string; label: string }> = [];
  for (const key of Object.keys(env)) {
    const meta = vars.find((v) => v.name === key);
    if (meta?.token_url) {
      out.push({ name: key, url: meta.token_url, label: meta.token_url_label || "Get token" });
    }
  }
  return out;
}
