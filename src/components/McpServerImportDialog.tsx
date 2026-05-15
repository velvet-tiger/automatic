import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X, ClipboardPaste, Loader2, CheckCircle2, AlertTriangle, FileText, ChevronDown } from "lucide-react";
import { trackMcpServerCreated } from "../lib/analytics";

/**
 * Names of MCP servers that are managed by Automatic itself and must not be
 * created or overwritten from a pasted snippet.
 */
const RESERVED_NAMES = new Set(["automatic"]);

/** Matches the Rust `is_valid_name` rule in src-tauri/src/core/paths.rs. */
function isValidName(name: string): boolean {
  return name.length > 0 && !name.includes("/") && !name.includes("\\") && name !== "." && name !== "..";
}

/**
 * Normalize a name for similarity matching: strip every non-alphanumeric
 * character and lowercase. Lets `betterstack`, `better-stack`, and
 * `Better_Stack` collapse to the same key so we can flag logical duplicates
 * that live under different filenames.
 */
function normalizeForMatch(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]/g, "");
}

type ConflictAction = "skip" | "overwrite" | "rename";

interface ParsedEntry {
  /** Name as it appeared in the snippet, or empty string for a single bare config. */
  originalName: string;
  /** Cleaned server config object to be saved. */
  config: Record<string, unknown>;
  /** Selected action when the original name collides with an existing server. */
  action: ConflictAction;
  /** When action === "rename", the new name the user has typed. */
  renameTo: string;
}

interface ParseResult {
  entries: ParsedEntry[];
  /** Set when the snippet is one bare server config with no name. */
  requiresName: boolean;
  /** Parse / structural error to show inline. */
  error: string | null;
}

interface ImportOutcome {
  imported: string[];
  failed: { name: string; error: string }[];
}

interface McpServerImportDialogProps {
  isOpen: boolean;
  /** Names already present in the Library — used for conflict detection. */
  existingNames: string[];
  onClose: () => void;
  /** Called with the names of all successfully imported servers. */
  onImported: (names: string[]) => void;
}

/** Server config fields we copy through from a pasted snippet. */
const PASSTHROUGH_FIELDS = ["type", "command", "args", "env", "cwd", "url", "headers", "oauth", "enabled", "timeout"];

/**
 * Build a clean server config from a raw parsed object: drop unknown fields,
 * drop the Automatic-internal `_builtin` marker, and infer `type` when absent.
 */
function cleanServerConfig(raw: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const key of PASSTHROUGH_FIELDS) {
    if (raw[key] !== undefined) out[key] = raw[key];
  }
  // Preserve _author so provider attribution survives a paste round-trip.
  if (raw["_author"] !== undefined) out["_author"] = raw["_author"];

  if (typeof out.type !== "string") {
    if (typeof out.command === "string") out.type = "stdio";
    else if (typeof out.url === "string") out.type = "http";
  }
  return out;
}

/**
 * Validate the shape of a server config. Returns an error string when the
 * config cannot be saved.
 */
function validateConfigShape(config: Record<string, unknown>): string | null {
  const type = config.type;
  if (type !== undefined && type !== "stdio" && type !== "http" && type !== "sse") {
    return `Unsupported transport type: "${String(type)}"`;
  }
  const hasCommand = typeof config.command === "string" && (config.command as string).length > 0;
  const hasUrl = typeof config.url === "string" && (config.url as string).length > 0;
  if (!hasCommand && !hasUrl) {
    return "Config must include either a 'command' (stdio) or a 'url' (http/sse).";
  }
  if (config.args !== undefined && !Array.isArray(config.args)) {
    return "'args' must be an array of strings.";
  }
  if (config.args !== undefined) {
    for (const a of config.args as unknown[]) {
      if (typeof a !== "string") return "'args' must be an array of strings.";
    }
  }
  if (config.env !== undefined && (typeof config.env !== "object" || config.env === null || Array.isArray(config.env))) {
    return "'env' must be an object of string→string.";
  }
  if (config.headers !== undefined && (typeof config.headers !== "object" || config.headers === null || Array.isArray(config.headers))) {
    return "'headers' must be an object of string→string.";
  }
  return null;
}

/** Detects whether an object looks like a bare single server config. */
function looksLikeBareConfig(obj: Record<string, unknown>): boolean {
  return ("type" in obj) || ("command" in obj) || ("url" in obj) || ("args" in obj);
}

/**
 * Parse a pasted MCP JSON snippet into a list of entries plus optional error.
 * Accepts three input shapes — see the plan for details.
 */
function parseSnippet(text: string): ParseResult {
  const trimmed = text.trim();
  if (trimmed.length === 0) {
    return { entries: [], requiresName: false, error: null };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch (e) {
    return { entries: [], requiresName: false, error: `Invalid JSON: ${(e as Error).message}` };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { entries: [], requiresName: false, error: "Expected a JSON object." };
  }

  const top = parsed as Record<string, unknown>;

  // Shape 1: { "mcpServers": { ... } }
  if (top.mcpServers && typeof top.mcpServers === "object" && !Array.isArray(top.mcpServers)) {
    const map = top.mcpServers as Record<string, unknown>;
    return buildEntriesFromMap(map);
  }

  // Shape 3: single bare config (no name).
  if (looksLikeBareConfig(top)) {
    const config = cleanServerConfig(top);
    return {
      entries: [{ originalName: "", config, action: "skip", renameTo: "" }],
      requiresName: true,
      error: null,
    };
  }

  // Shape 2: bare name→config map.
  return buildEntriesFromMap(top);
}

function buildEntriesFromMap(map: Record<string, unknown>): ParseResult {
  const entries: ParsedEntry[] = [];
  for (const [name, value] of Object.entries(map)) {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
      // Surface as an entry with a name but an unusable config so the user
      // can see why it cannot be imported.
      entries.push({
        originalName: name,
        config: {},
        action: "skip",
        renameTo: "",
      });
      continue;
    }
    entries.push({
      originalName: name,
      config: cleanServerConfig(value as Record<string, unknown>),
      action: "skip",
      renameTo: "",
    });
  }
  if (entries.length === 0) {
    return { entries: [], requiresName: false, error: "No servers found in snippet." };
  }
  return { entries, requiresName: false, error: null };
}

interface RowComputed {
  /** Final name that will be used if this row is saved. */
  effectiveName: string;
  /** True when the original name collides with an existing Library entry. */
  conflicts: boolean;
  /**
   * Existing library entry that matched the pasted name under slug-insensitive
   * comparison. Only set when there is a conflict. Equals `entry.originalName`
   * for exact matches; differs for slug-only matches (e.g. `better-stack`
   * matching pasted `betterstack`).
   */
  matchedExistingName: string | null;
  /** Validation error preventing import, or null when row is importable. */
  error: string | null;
  /** True when the row will be sent to the backend on Import. */
  willImport: boolean;
}

function computeRow(
  entry: ParsedEntry,
  existing: Map<string, string>,
  singleBareName: string,
  requiresName: boolean,
): RowComputed {
  const effectiveName = requiresName
    ? singleBareName.trim()
    : entry.action === "rename"
      ? entry.renameTo.trim()
      : entry.originalName;

  const matchedExistingName = requiresName
    ? null
    : existing.get(normalizeForMatch(entry.originalName)) ?? null;
  const conflicts = matchedExistingName !== null;

  // Config shape errors apply regardless of name choice.
  const shapeError = validateConfigShape(entry.config);
  if (shapeError) {
    return { effectiveName, conflicts, matchedExistingName, error: shapeError, willImport: false };
  }

  if (effectiveName.length === 0) {
    return {
      effectiveName,
      conflicts,
      matchedExistingName,
      error: requiresName ? "Enter a name for this server." : "Rename to a non-empty value.",
      willImport: false,
    };
  }
  if (!isValidName(effectiveName)) {
    return {
      effectiveName,
      conflicts,
      matchedExistingName,
      error: `Invalid name "${effectiveName}". Names cannot contain '/' or '\\' and cannot be '.' or '..'.`,
      willImport: false,
    };
  }
  if (RESERVED_NAMES.has(effectiveName)) {
    return {
      effectiveName,
      conflicts,
      matchedExistingName,
      error: `"${effectiveName}" is reserved for the built-in Automatic server.`,
      willImport: false,
    };
  }

  // Conflict handling.
  if (conflicts && entry.action === "skip") {
    return { effectiveName, conflicts, matchedExistingName, error: null, willImport: false };
  }
  if (conflicts && entry.action === "rename") {
    // A renamed entry must not collide with any existing library name under
    // slug-insensitive comparison either — otherwise the user is just moving
    // the conflict, not resolving it.
    const renameCollision = existing.get(normalizeForMatch(effectiveName));
    if (renameCollision !== undefined && renameCollision !== matchedExistingName) {
      return {
        effectiveName,
        conflicts,
        matchedExistingName,
        error: `"${effectiveName}" already exists in the Library — pick a different name.`,
        willImport: false,
      };
    }
  }

  return { effectiveName, conflicts, matchedExistingName, error: null, willImport: true };
}

export default function McpServerImportDialog({
  isOpen,
  existingNames,
  onClose,
  onImported,
}: McpServerImportDialogProps) {
  const [text, setText] = useState("");
  const [entries, setEntries] = useState<ParsedEntry[]>([]);
  const [requiresName, setRequiresName] = useState(false);
  const [parseError, setParseError] = useState<string | null>(null);
  const [singleBareName, setSingleBareName] = useState("");
  const [importing, setImporting] = useState(false);
  const [outcome, setOutcome] = useState<ImportOutcome | null>(null);

  // Indexed by the slug-insensitive form of each existing name. The value
  // preserves the original (display) name so the UI can render `Exists as
  // <name>` when a paste matches under normalization but not exact spelling.
  const existing = useMemo(() => {
    const map = new Map<string, string>();
    for (const name of existingNames) {
      map.set(normalizeForMatch(name), name);
    }
    return map;
  }, [existingNames]);

  // Re-parse whenever the pasted text changes.
  useEffect(() => {
    const result = parseSnippet(text);
    setEntries(result.entries);
    setRequiresName(result.requiresName);
    setParseError(result.error);
    setSingleBareName("");
    setOutcome(null);
  }, [text]);

  const rows = entries.map((entry) => computeRow(entry, existing, singleBareName, requiresName));
  const importableCount = rows.filter((r) => r.willImport).length;

  const reset = () => {
    setText("");
    setEntries([]);
    setRequiresName(false);
    setParseError(null);
    setSingleBareName("");
    setOutcome(null);
  };

  const handleClose = () => {
    if (importing) return;
    reset();
    onClose();
  };

  const updateEntry = (index: number, patch: Partial<ParsedEntry>) => {
    setEntries((prev) => prev.map((e, i) => (i === index ? { ...e, ...patch } : e)));
  };

  const handleImport = async () => {
    setImporting(true);
    const imported: string[] = [];
    const failed: { name: string; error: string }[] = [];

    for (let i = 0; i < entries.length; i++) {
      const row = rows[i]!;
      if (!row.willImport) continue;
      const entry = entries[i]!;
      const name = row.effectiveName;
      try {
        await invoke("save_mcp_server_config", {
          name,
          data: JSON.stringify(entry.config),
        });
        trackMcpServerCreated(name);
        imported.push(name);
      } catch (err: unknown) {
        failed.push({ name, error: String(err) });
      }
    }

    setImporting(false);
    setOutcome({ imported, failed });

    if (imported.length > 0) {
      onImported(imported);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={handleClose} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-2xl mx-4 max-h-[85vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40">
          <div className="flex items-center gap-2">
            <ClipboardPaste size={16} className="text-text-muted" />
            <h2 className="text-[15px] font-semibold text-text-base">Import MCP Server From JSON</h2>
          </div>
          <button
            onClick={handleClose}
            disabled={importing}
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50"
          >
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          <p className="text-[13px] text-text-muted leading-relaxed">
            Paste an MCP configuration snippet. Standard <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">mcpServers</code> wrappers,
            bare name → config maps, and single server configs are all accepted.
          </p>

          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={`{\n  "mcpServers": {\n    "endor-cli-tools": {\n      "type": "stdio",\n      "command": "npx",\n      "args": ["-y", "endorctl", "ai-tools", "mcp-server"]\n    }\n  }\n}`}
            spellCheck={false}
            className="w-full h-48 px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[12px] text-text-base placeholder-text-muted/40 font-mono resize-y"
          />

          {parseError && (
            <div className="flex items-start gap-2 rounded-lg border border-danger/25 bg-danger/8 px-3 py-2">
              <AlertTriangle size={12} className="text-danger mt-0.5 shrink-0" />
              <p className="text-[12px] text-danger">{parseError}</p>
            </div>
          )}

          {requiresName && entries.length === 1 && (
            <div>
              <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1.5">
                Server Name <span className="text-danger">*</span>
              </label>
              <input
                type="text"
                value={singleBareName}
                onChange={(e) => setSingleBareName(e.target.value)}
                placeholder="e.g. endor-cli-tools"
                spellCheck={false}
                className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[13px] text-text-base placeholder-text-muted/40 font-mono"
              />
              <p className="mt-1 text-[11px] text-text-muted">
                Snippet did not include a name — give the server one before importing.
              </p>
            </div>
          )}

          {entries.length > 0 && !outcome && (
            <div className="space-y-2">
              <p className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                {entries.length === 1 ? "Detected server" : `Detected ${entries.length} servers`}
              </p>
              <ul className="space-y-2">
                {entries.map((entry, i) => {
                  const row = rows[i]!;
                  const transport = (entry.config.type as string) || "—";
                  const summary =
                    transport === "stdio"
                      ? `${(entry.config.command as string) || "?"} ${((entry.config.args as string[]) || []).join(" ")}`.trim()
                      : (entry.config.url as string) || "";
                  return (
                    <li
                      key={i}
                      className={`rounded-lg border px-3 py-2.5 ${
                        row.error
                          ? "border-danger/25 bg-danger/8"
                          : row.willImport
                            ? "border-border-strong/40 bg-bg-sidebar/40"
                            : "border-border-strong/30 bg-bg-sidebar/20"
                      }`}
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className="font-mono text-[12px] text-text-base">
                              {row.effectiveName || entry.originalName || "(unnamed)"}
                            </span>
                            <span className="text-[10px] uppercase tracking-wider text-text-muted bg-bg-base/40 px-1.5 py-0.5 rounded">
                              {transport}
                            </span>
                            {row.conflicts && (
                              <span className="text-[10px] uppercase tracking-wider text-warning bg-warning/10 px-1.5 py-0.5 rounded">
                                {row.matchedExistingName && row.matchedExistingName !== entry.originalName
                                  ? `Exists as ${row.matchedExistingName}`
                                  : "Exists"}
                              </span>
                            )}
                          </div>
                          {summary && (
                            <p className="mt-1 text-[11px] text-text-muted font-mono truncate" title={summary}>
                              {summary}
                            </p>
                          )}
                          {row.error && (
                            <p className="mt-1 text-[11px] text-danger">{row.error}</p>
                          )}
                        </div>

                        {row.conflicts && !requiresName && (() => {
                          // Overwrite only makes sense for exact-name conflicts —
                          // a slug-only match would write to a different filename
                          // than the matched existing entry, leaving both in place.
                          const exactMatch = row.matchedExistingName === entry.originalName;
                          return (
                            <div className="relative shrink-0">
                              <select
                                value={entry.action}
                                onChange={(e) => updateEntry(i, { action: e.target.value as ConflictAction })}
                                className="h-7 min-w-[110px] appearance-none rounded-md border border-border-strong/50 bg-bg-input px-2.5 pr-7 text-[12px] text-text-base shadow-none focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60"
                              >
                                <option value="skip">Skip</option>
                                {exactMatch && <option value="overwrite">Overwrite</option>}
                                <option value="rename">Rename…</option>
                              </select>
                              <ChevronDown
                                size={12}
                                className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted"
                              />
                            </div>
                          );
                        })()}
                      </div>

                      {row.conflicts && entry.action === "rename" && !requiresName && (
                        <input
                          type="text"
                          value={entry.renameTo}
                          onChange={(e) => updateEntry(i, { renameTo: e.target.value })}
                          placeholder="new-server-name"
                          spellCheck={false}
                          className="mt-2 w-full px-2 py-1.5 rounded-md bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[12px] text-text-base placeholder-text-muted/40 font-mono"
                        />
                      )}
                    </li>
                  );
                })}
              </ul>
            </div>
          )}

          {outcome && (
            <div className="space-y-2">
              {outcome.imported.length > 0 && (
                <div className="rounded-lg border border-success/20 bg-success/10 px-3 py-2">
                  <div className="flex items-center gap-2">
                    <CheckCircle2 size={14} className="text-success" />
                    <span className="text-[12px] font-medium text-success">
                      Imported {outcome.imported.length} server{outcome.imported.length === 1 ? "" : "s"}
                    </span>
                  </div>
                  <ul className="mt-1 space-y-1">
                    {outcome.imported.map((n) => (
                      <li key={n} className="flex items-center gap-2 text-[12px] text-text-muted">
                        <FileText size={12} className="text-success" />
                        <span className="font-mono">{n}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              {outcome.failed.length > 0 && (
                <div className="rounded-lg border border-danger/25 bg-danger/8 px-3 py-2">
                  <div className="flex items-center gap-2 mb-1">
                    <AlertTriangle size={14} className="text-danger" />
                    <span className="text-[12px] font-medium text-danger">
                      {outcome.failed.length} failed
                    </span>
                  </div>
                  <ul className="space-y-1">
                    {outcome.failed.map((f, idx) => (
                      <li key={idx} className="text-[11px] text-danger">
                        <span className="font-mono">{f.name}</span>: {f.error}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end items-center gap-2 px-5 py-3 border-t border-border-strong/40">
          <button
            onClick={handleClose}
            disabled={importing}
            className="px-4 py-2 text-[13px] font-medium text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded-lg transition-colors disabled:opacity-50"
          >
            {outcome && outcome.imported.length > 0 && outcome.failed.length === 0 ? "Done" : "Cancel"}
          </button>
          <button
            onClick={handleImport}
            disabled={importing || importableCount === 0}
            className="px-4 py-2 text-[13px] font-medium bg-brand hover:bg-brand-hover text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2"
          >
            {importing ? (
              <>
                <Loader2 size={14} className="animate-spin" />
                Importing…
              </>
            ) : (
              <>Import {importableCount > 0 ? `${importableCount} server${importableCount === 1 ? "" : "s"}` : ""}</>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
