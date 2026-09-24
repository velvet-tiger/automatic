// Types mirroring `src-tauri/src/core/contexts.rs` and `context_sources.rs`.
// The serde shapes are: a context carries `location` ("local" | "cloud")
// flattened beside its fields, and a source carries `kind` with an optional
// `config` object (the webapp's source shape).

export interface LocalSourceConfig {
  /** Absolute path to a file or directory. */
  path: string;
  /** Glob patterns relative to `path`. Omitted or empty means the default text set. */
  include?: string[];
}

export interface UrlSourceConfig {
  url: string;
  /** Seconds a fetched copy stays fresh. 0 fetches on every read. */
  ttl_secs: number;
}

export interface CloudSourceConfig {
  /** Webapp source id (`src_...`). */
  source_id: string;
}

interface SourceBase {
  id: string;
  display_name: string;
  description: string;
}

export type ContextSource =
  | (SourceBase & { kind: "documentation" })
  | (SourceBase & { kind: "local"; config: LocalSourceConfig })
  | (SourceBase & { kind: "url"; config: UrlSourceConfig })
  | (SourceBase & { kind: "cloud"; config: CloudSourceConfig });

export type SourceKind = ContextSource["kind"];

interface ContextBase {
  slug: string;
  display_name: string;
  description: string;
  created_at: string;
  updated_at: string;
}

export type Context =
  | (ContextBase & { location: "local"; sources: ContextSource[] })
  | (ContextBase & { location: "cloud"; context_id: string });

export type ContextLocation = Context["location"];

export interface SourceSummary {
  id: string;
  kind: string;
  display_name?: string;
  description?: string;
}

export interface SourceEntry {
  path: string;
  title?: string;
  size?: number;
}

export interface SourceListing {
  entries: SourceEntry[];
  truncated?: boolean;
}

export interface ContextReferences {
  projects: string[];
  groups: string[];
}

/** Matches `core::ContextTarget` (`{"type": "project", "name": "..."}`). */
export type ContextTarget =
  | { type: "project"; name: string }
  | { type: "group"; name: string };

export const SOURCE_KIND_LABELS: Record<SourceKind, string> = {
  documentation: "Documentation",
  local: "Local files",
  url: "URL",
  cloud: "Cloud source",
};

export const DEFAULT_URL_TTL_SECS = 3600;
export const DEFAULT_LOCAL_INCLUDE = ["**/*.md", "**/*.mdx", "**/*.txt"];
export const SLUG_MAX_LENGTH = 128;

/** Same grammar as `is_valid_context_slug` in Rust. */
export function isValidSlug(slug: string): boolean {
  return slug.length <= SLUG_MAX_LENGTH && /^[a-z0-9][a-z0-9._-]*$/.test(slug);
}

/**
 * Derive a slug from a display name, as the webapp does. Returns "" when
 * nothing valid survives so the caller asks for an explicit slug.
 */
export function deriveSlug(displayName: string): string {
  const slug = displayName
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^[^a-z0-9]+/, "")
    .replace(/-+$/, "")
    .slice(0, SLUG_MAX_LENGTH);
  return isValidSlug(slug) ? slug : "";
}

/** A source id derived from `name`, unique among `sources`. Ids are never shown. */
export function uniqueSourceId(name: string, sources: ContextSource[]): string {
  const taken = new Set(sources.map((s) => s.id));
  const base = deriveSlug(name) || "source";
  let candidate = base;
  let i = 2;
  while (taken.has(candidate)) candidate = `${base}-${i++}`;
  return candidate;
}

export type LinkedSource = Exclude<ContextSource, { kind: "documentation" }>;
export type DocumentationSource = Extract<ContextSource, { kind: "documentation" }>;

export function isLinkedSource(source: ContextSource): source is LinkedSource {
  return source.kind !== "documentation";
}

export function isDocumentationSource(source: ContextSource): source is DocumentationSource {
  return source.kind === "documentation";
}

/** The id of the built-in pages source a new context gets. */
export const PAGES_SOURCE_ID = "pages";

export function summariseContext(context: Context): string {
  if (context.location === "cloud") return "From Automatic cloud";
  const linked = context.sources.filter((s) => s.kind !== "documentation").length;
  return linked === 0 ? "Pages" : `Pages and ${linked} linked item${linked === 1 ? "" : "s"}`;
}

/** Which group provides `slug` to a project, if any. */
export function providingGroup(
  contributions: Record<string, string[]> | undefined,
  slug: string,
): string | null {
  const groups = Object.entries(contributions ?? {})
    .filter(([, slugs]) => slugs.includes(slug))
    .map(([group]) => group)
    .sort();
  return groups[0] ?? null;
}

/**
 * Early feedback for one source, mirroring the backend's checks. The
 * backend validates again on save; this only saves a round trip.
 */
export function sourceProblem(source: ContextSource, sources: ContextSource[]): string | null {
  if (!isValidSlug(source.id)) {
    return "Source id: use lowercase letters, digits, '.', '_' and '-', starting with a letter or digit.";
  }
  if (sources.filter((s) => s.id === source.id).length > 1) {
    return `Another source already uses the id "${source.id}".`;
  }
  switch (source.kind) {
    case "documentation":
      return null;
    case "local":
      return /^(\/|[A-Za-z]:[\\/]|\\\\)/.test(source.config.path)
        ? null
        : "Choose an absolute folder or file path.";
    case "url":
      if (!/^https?:\/\//i.test(source.config.url)) return "Enter an http or https URL.";
      if (/^https?:\/\/[^/]*@/i.test(source.config.url)) {
        return "URLs with a user name or password are not supported.";
      }
      return null;
    case "cloud":
      return /^src_[A-Za-z0-9_-]+$/.test(source.config.source_id)
        ? null
        : "Enter a cloud source id that starts with src_.";
  }
}
