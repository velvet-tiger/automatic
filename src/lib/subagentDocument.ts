import { nextAvailableName } from "./uniqueName";
import { escapeYamlDoubleQuoted } from "./yaml";

/**
 * Split a sub-agent Markdown file into editable front matter fields and a
 * prompt body, then put it back together.
 *
 * The round trip is lossless. Front matter keys the editor does not know
 * (`hooks`, `permissionMode`, comments, and so on) are kept as raw YAML text.
 * A known key whose value has not changed is written back byte for byte, so
 * quoting, block scalars, and list syntax survive an edit to another field.
 * Only the keys the user changed are re-encoded.
 *
 * This is a line-level splitter, not a YAML parser. It finds top-level keys
 * (lines that start in column 0 with `key:`) and treats every following
 * indented, blank, or comment line as part of that key's entry.
 */

/** Front matter keys shown as their own fields, in the order they are added. */
export const SUBAGENT_FIELD_KEYS = ["name", "description", "tools", "model", "color"] as const;

export type SubagentFieldKey = (typeof SUBAGENT_FIELD_KEYS)[number];

/** Field values. An empty string means the key is absent from the file. */
export type SubagentFields = Record<SubagentFieldKey, string>;

/** One top-level front matter entry: its key line plus continuation lines. */
interface FrontmatterEntry {
  /** `null` for comment or blank lines that come before the first key. */
  key: string | null;
  lines: string[];
}

/** The original shape of the file, used to write unchanged parts back verbatim. */
interface FrontmatterLayout {
  hadFrontmatter: boolean;
  newline: string;
  entries: FrontmatterEntry[];
  /** Keys whose entry was lifted into `fields`. Every other entry is "extra". */
  fieldKeys: ReadonlySet<SubagentFieldKey>;
  /** The closing `---` line exactly as written, including its line break. */
  closing: string;
}

export interface SubagentDocument {
  fields: SubagentFields;
  /** Raw YAML for every front matter entry that is not one of `fields`. */
  extraFrontmatter: string;
  /** The prompt: everything after the closing `---` line. */
  body: string;
  layout: FrontmatterLayout;
}

const TOP_LEVEL_KEY_RE = /^([A-Za-z0-9_][A-Za-z0-9_.-]*)[ \t]*:(?:[ \t]|$)/;

function isFieldKey(key: string): key is SubagentFieldKey {
  return (SUBAGENT_FIELD_KEYS as readonly string[]).includes(key);
}

function emptyFields(): SubagentFields {
  return { name: "", description: "", tools: "", model: "", color: "" };
}

function splitEntries(lines: string[]): FrontmatterEntry[] {
  const entries: FrontmatterEntry[] = [];
  for (const line of lines) {
    const match = TOP_LEVEL_KEY_RE.exec(line);
    const last = entries[entries.length - 1];
    if (match) {
      entries.push({ key: match[1]!, lines: [line] });
    } else if (last) {
      last.lines.push(line);
    } else {
      entries.push({ key: null, lines: [line] });
    }
  }
  return entries;
}

function isBlankOrComment(line: string): boolean {
  const trimmed = line.trim();
  return trimmed === "" || trimmed.startsWith("#");
}

/** Trailing blank or comment lines belong to the layout, not to the value. */
function splitTrailing(lines: string[]): { valueLines: string[]; trailing: string[] } {
  let end = lines.length;
  while (end > 1 && isBlankOrComment(lines[end - 1]!)) end--;
  return { valueLines: lines.slice(0, end), trailing: lines.slice(end) };
}

function stripIndent(lines: string[]): string[] {
  const indents = lines
    .filter(l => l.trim() !== "")
    .map(l => l.length - l.trimStart().length);
  const min = indents.length > 0 ? Math.min(...indents) : 0;
  return lines.map(l => l.slice(min).trimEnd());
}

function unquoteDouble(inner: string): string {
  return inner.replace(/\\(.)/g, (_m, ch: string) => {
    if (ch === "n") return "\n";
    if (ch === "t") return "\t";
    if (ch === "r") return "\r";
    return ch;
  });
}

function unquoteScalar(text: string): string {
  const t = text.trim();
  if (t.length >= 2 && t.startsWith('"') && t.endsWith('"')) return unquoteDouble(t.slice(1, -1));
  if (t.length >= 2 && t.startsWith("'") && t.endsWith("'")) return t.slice(1, -1).replace(/''/g, "'");
  return t;
}

function stripPlainComment(text: string): string {
  const idx = text.search(/\s#/);
  return idx === -1 ? text : text.slice(0, idx);
}

/**
 * Decode one entry into a single-line or multi-line string. Returns `null`
 * when the value is a nested mapping or anything else a plain text field
 * cannot represent; the caller then keeps the entry as extra front matter.
 */
function decodeEntry(entry: FrontmatterEntry): string | null {
  const { valueLines } = splitTrailing(entry.lines);
  const first = valueLines[0]!;
  const rest = first.slice(first.indexOf(":") + 1).trim();
  const continuation = valueLines.slice(1);
  const cont = continuation.filter(l => l.trim() !== "" && !l.trim().startsWith("#"));

  // Block scalar: `key: |`, `key: >-`, and so on.
  if (/^[|>][+-]?\d*$/.test(rest)) {
    const body = stripIndent(continuation);
    while (body.length > 0 && body[body.length - 1] === "") body.pop();
    if (rest.startsWith("|")) return body.join("\n");
    // Folded: single line breaks become spaces, blank lines become breaks.
    return body.reduce((acc, line) => {
      if (line === "") return `${acc}\n`;
      if (acc === "" || acc.endsWith("\n")) return acc + line;
      return `${acc} ${line}`;
    }, "");
  }

  if (rest === "") {
    if (cont.length === 0) return "";
    // Block sequence (`- Read`) becomes the comma-separated form the field shows.
    if (cont.every(l => /^\s*-(\s|$)/.test(l))) {
      return cont.map(l => unquoteScalar(l.trim().slice(1))).join(", ");
    }
    return null;
  }

  if (rest.startsWith("[")) {
    const joined = [rest, ...cont.map(l => l.trim())].join(" ");
    if (!joined.endsWith("]")) return null;
    return joined
      .slice(1, -1)
      .split(",")
      .map(item => unquoteScalar(item))
      .filter(item => item !== "")
      .join(", ");
  }

  if (rest.startsWith("{")) return null;

  if (rest.startsWith('"') || rest.startsWith("'")) {
    return unquoteScalar([rest, ...cont.map(l => l.trim())].join(" "));
  }

  // Multi-line plain scalars fold onto one line.
  return [stripPlainComment(rest), ...cont.map(l => l.trim())].join(" ").trim();
}

const YAML_RESERVED_PLAIN_RE = /^(true|false|yes|no|on|off|null|~)$/i;
const YAML_NUMBER_RE = /^[-+]?(\.?\d[\d_.eE+-]*|\.inf|\.nan|0x[0-9a-f]+|0o[0-7]+)$/i;

function needsQuotes(value: string): boolean {
  return (
    value !== value.trim() ||
    /[\n\r\t]/.test(value) ||
    /^[-?:,[\]{}#&*!|>'"%@`]/.test(value) ||
    /: |:$| #/.test(value) ||
    YAML_RESERVED_PLAIN_RE.test(value) ||
    YAML_NUMBER_RE.test(value)
  );
}

function encodeScalar(value: string): string {
  if (!needsQuotes(value)) return value;
  const escaped = escapeYamlDoubleQuoted(value)
    .replace(/\n/g, "\\n")
    .replace(/\r/g, "\\r")
    .replace(/\t/g, "\\t");
  return `"${escaped}"`;
}

/** Parse a sub-agent file into fields, extra front matter, and prompt body. */
export function parseSubagent(content: string): SubagentDocument {
  const newline = content.startsWith("---\r\n") ? "\r\n" : "\n";
  const noFrontmatter: SubagentDocument = {
    fields: emptyFields(),
    extraFrontmatter: "",
    body: content,
    layout: { hadFrontmatter: false, newline: "\n", entries: [], fieldKeys: new Set(), closing: "---\n" },
  };
  if (!content.startsWith(`---${newline}`)) return noFrontmatter;

  const afterOpen = content.slice(3 + newline.length);
  const lines = afterOpen.split(newline);
  const closeIdx = lines.findIndex(l => l.trimEnd() === "---");
  if (closeIdx === -1) return noFrontmatter;

  const entries = splitEntries(lines.slice(0, closeIdx));
  const fields = emptyFields();
  const fieldKeys = new Set<SubagentFieldKey>();
  for (const entry of entries) {
    // Only the first occurrence of a key is editable. A duplicate stays extra
    // so it is kept rather than silently dropped.
    if (entry.key === null || !isFieldKey(entry.key) || fieldKeys.has(entry.key)) continue;
    const value = decodeEntry(entry);
    if (value === null) continue;
    fields[entry.key] = value;
    fieldKeys.add(entry.key);
  }

  const isLast = closeIdx === lines.length - 1;
  const closing = isLast ? lines[closeIdx]! : lines[closeIdx]! + newline;
  const body = lines.slice(closeIdx + 1).join(newline);

  const layout: FrontmatterLayout = { hadFrontmatter: true, newline, entries, fieldKeys, closing };
  return {
    fields,
    extraFrontmatter: extraEntries(layout).flatMap(e => e.lines).join("\n"),
    body,
    layout,
  };
}

function isFieldEntry(layout: FrontmatterLayout, entry: FrontmatterEntry): boolean {
  if (entry.key === null || !isFieldKey(entry.key) || !layout.fieldKeys.has(entry.key)) return false;
  // Duplicates of a field key are extra; only the first occurrence is the field.
  return layout.entries.find(e => e.key === entry.key) === entry;
}

function extraEntries(layout: FrontmatterLayout): FrontmatterEntry[] {
  return layout.entries.filter(e => !isFieldEntry(layout, e));
}

/**
 * Field keys that are set as a field and also appear in the extra front
 * matter. Saving in that state would write the key twice.
 */
export function conflictingExtraKeys(doc: SubagentDocument): SubagentFieldKey[] {
  const keys = new Set<SubagentFieldKey>();
  for (const entry of splitEntries(doc.extraFrontmatter.split(/\r?\n/))) {
    if (entry.key !== null && isFieldKey(entry.key) && doc.fields[entry.key].trim() !== "") {
      keys.add(entry.key);
    }
  }
  return SUBAGENT_FIELD_KEYS.filter(k => keys.has(k));
}

/** Rebuild the sub-agent file from its fields, extra front matter, and body. */
export function serializeSubagent(doc: SubagentDocument): string {
  const { layout, fields } = doc;
  const nl = layout.newline;
  const originalExtras = extraEntries(layout);
  const extrasUnchanged = doc.extraFrontmatter === originalExtras.flatMap(e => e.lines).join("\n");

  const out: string[] = [];
  const written = new Set<SubagentFieldKey>();

  for (const entry of layout.entries) {
    if (isFieldEntry(layout, entry)) {
      const key = entry.key as SubagentFieldKey;
      written.add(key);
      const value = fields[key];
      const { trailing } = splitTrailing(entry.lines);
      if (value.trim() === "") {
        out.push(...trailing);
      } else if (decodeEntry(entry) === value) {
        out.push(...entry.lines);
      } else {
        out.push(`${key}: ${encodeScalar(value)}`, ...trailing);
      }
    } else if (extrasUnchanged) {
      out.push(...entry.lines);
    }
  }

  // Newly filled fields go after the existing keys, in the canonical order.
  // An extra entry at the end with trailing blank lines would otherwise
  // absorb them, so they are inserted before that trailing run.
  const added = SUBAGENT_FIELD_KEYS
    .filter(key => !written.has(key) && fields[key].trim() !== "")
    .map(key => `${key}: ${encodeScalar(fields[key])}`);
  if (added.length > 0) {
    let insertAt = out.length;
    while (insertAt > 0 && out[insertAt - 1]!.trim() === "") insertAt--;
    out.splice(insertAt, 0, ...added);
  }

  if (!extrasUnchanged) {
    const extra = doc.extraFrontmatter.replace(/\s+$/, "");
    if (extra !== "") out.push(...extra.split(/\r?\n/));
  }

  // A file that had no front matter and still has none is left as it was.
  if (!layout.hadFrontmatter && out.length === 0) return doc.body;

  const frontmatter = out.length > 0 ? out.join(nl) + nl : "";
  const closing =
    doc.body !== "" && !layout.closing.endsWith("\n") ? layout.closing + nl : layout.closing;
  return `---${nl}${frontmatter}${closing}${doc.body}`;
}

export interface SubagentIdentity {
  /** Machine name: the library file stem. */
  id: string;
  /** Front matter `name`. */
  name: string;
}

/**
 * Pick the name and machine name for a copy of `source`. Both get a `-copy`
 * suffix, then `-copy-2` and so on, so the copy never shares a name with an
 * existing agent. Tools call a sub-agent by its name, so two agents with the
 * same name would shadow each other once synced.
 */
export function duplicateSubagentIdentity(
  source: SubagentIdentity,
  existing: readonly SubagentIdentity[],
): SubagentIdentity {
  return {
    id: nextAvailableName(subagentMachineName(`${source.id}-copy`), existing.map(e => e.id)),
    name: nextAvailableName(`${source.name}-copy`, existing.map(e => e.name)),
  };
}

/** Return `content` with its front matter `name` replaced, keeping everything else. */
export function withSubagentName(content: string, name: string): string {
  const doc = parseSubagent(content);
  return serializeSubagent({ ...doc, fields: { ...doc.fields, name } });
}

/**
 * Derive a library file name (machine name) from a sub-agent's name.
 * The result satisfies `is_valid_agent_machine_name` in `core/subagents.rs`:
 * lowercase letters, digits, and single hyphens, starting with a letter.
 */
export function subagentMachineName(name: string): string {
  let slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  // `automatic-` marks bundled, read-only agents; a user agent must not claim it.
  while (slug.startsWith("automatic-")) slug = slug.slice("automatic-".length);
  if (slug === "automatic") slug = "";
  if (slug === "") return "agent";
  if (!/^[a-z]/.test(slug)) slug = `agent-${slug}`;
  slug = slug.slice(0, 120).replace(/-+$/, "");
  // `codex-*-openai` marks discovered Codex agents, which are also read-only.
  if (/^codex-.*-openai$/.test(slug)) slug = `${slug}-agent`;
  return slug;
}
