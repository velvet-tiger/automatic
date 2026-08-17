import React, { useState, useEffect, useRef } from "react";
import { escapeYamlDoubleQuoted } from "../../lib/yaml";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { MarkdownPreview } from "../../components/MarkdownPreview";
import { LineNumberedTextarea } from "../../components/LineNumberedTextarea";
import { AuthorSection, type AuthorDescriptor } from "../../components/AuthorPanel";
import { handleExternalLinkClick } from "../../lib/externalLinks";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  trackSkillCreated,
  trackSkillUpdated,
  trackSkillDeleted,
} from "../../lib/analytics";
import {
  Plus,
  X,
  Edit2,
  Code,
  FileText,
  Check,
  Globe,
  Github,
  Search,
  FolderOpen,
  LayoutTemplate,
  Copy,
  Download,
  Tag,
  ChevronDown,
  Puzzle,
} from "lucide-react";
import { ICONS } from "../../lib/icons";
import { SkillAvatar } from "../../components/SkillAvatar";
import { TokenPill } from "../../components/TokenPill";
import SkillImportDialog from "../../components/SkillImportDialog";
import { AssetTable } from "../../components/AssetTable";
import { AssetDrawer } from "../../components/AssetDrawer";
import { BuiltInBadge, ReadOnlyBadge, LockCell } from "../../components/ProtectionBadge";
import { useBulkSelection } from "../../lib/useBulkSelection";
import {
  type AssetSecurityScanRecord,
  formatAssetScanResult,
  getAssetSecurityDismissButtonClass,
  getAssetSecurityNoticeClass,
  getAssetSecurityStatus,
  scanAssetContent,
  toAssetSecurityScanRecord,
  warningFindings,
} from "../../lib/assetSecurity";

interface SkillSource {
  source: string; // "owner/repo"
  id: string;     // "owner/repo/skill-name"
  kind?: string;  // "github" | "bundled"
  // Install-time metadata used by the "is this skill out of date?" check.
  // All optional — absent for skills imported before this feature shipped
  // and for bundled skills.
  installed_sha?: string;
  installed_version?: string;
  installed_at?: string;
}

// Mirrors `core::skill_store::SkillUpdateStatus` on the Rust side.
interface SkillUpdateStatus {
  status: "up_to_date" | "update_available" | "local_modified" | "unknown";
  installed_sha?: string;
  local_sha?: string;
  remote_sha?: string;
  installed_version?: string;
  remote_version?: string;
  installed_at?: string;
  reason?: string;
}

interface SkillEntry {
  name: string;
  sources: string[]; // e.g., ["agents", "claude", "codex", "cline"]
  source?: SkillSource;
  has_resources: boolean;
  license?: string;
  plugin_id?: string;
  collection?: string;
}

interface SkillCollection {
  name: string;
  skills: string[];
}

interface SkillUsedBy {
  projects: string[];
  templates: string[];
}

interface ResourceFile {
  path: string;
}

interface ResourceDir {
  name: string;
  files: ResourceFile[];
}

interface SkillResources {
  dirs: ResourceDir[];
  root_files: ResourceFile[];
}

// ── Frontmatter parser (same as SkillStore) ──────────────────────────────────

interface Frontmatter {
  name?: string;
  description?: string;
  [key: string]: string | undefined;
}

function parseFrontmatter(raw: string): { meta: Frontmatter; body: string } {
  const match = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!match) return { meta: {}, body: raw };

  const meta: Frontmatter = {};
  const lines = match[1]!.split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i]!;
    const colonIdx = line.indexOf(":");
    if (colonIdx === -1) { i++; continue; }

    const key = line.slice(0, colonIdx).trim();
    const rest = line.slice(colonIdx + 1).trim();

    if (!key) { i++; continue; }

    // YAML block scalar: `key: >`, `key: >-`, `key: |`, `key: |-`
    if (rest === ">" || rest === ">-" || rest === "|" || rest === "|-") {
      i++;
      const blockLines: string[] = [];
      while (i < lines.length && (lines[i]!.startsWith(" ") || lines[i]!.startsWith("\t"))) {
        blockLines.push(lines[i]!.trim());
        i++;
      }
      meta[key] = blockLines.join(rest.startsWith("|") ? "\n" : " ");
    } else {
      meta[key] = rest.replace(/^["']|["']$/g, "");
      i++;
    }
  }

  return { meta, body: match[2]!.trimStart() };
}

// ── Remote update badge ───────────────────────────────────────────────────────
//
// Rendered when a GitHub-sourced skill's local copy can be compared against
// the upstream. We deliberately stay quiet for "unknown" — the user didn't
// ask for an update check, so a noisy "couldn't verify" message would be
// worse than nothing.

function formatInstalledAt(iso?: string): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function SkillUpdateBadge({ status }: { status: SkillUpdateStatus }) {
  const installedAt = formatInstalledAt(status.installed_at);
  const versionPair =
    status.installed_version && status.remote_version && status.installed_version !== status.remote_version
      ? `${status.installed_version} → ${status.remote_version}`
      : null;

  if (status.status === "up_to_date") {
    return (
      <div className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-medium bg-bg-sidebar border border-border-strong/40 text-text-muted">
        <Check size={11} className="text-emerald-500" />
        <span>Up to date with source</span>
        {installedAt && (
          <span className="text-text-muted/70">· installed {installedAt}</span>
        )}
      </div>
    );
  }

  if (status.status === "update_available") {
    return (
      <div className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-medium bg-warning/10 border border-warning/30 text-warning">
        <Download size={11} />
        <span>Update available{versionPair ? ` (${versionPair})` : ""}</span>
      </div>
    );
  }

  if (status.status === "local_modified") {
    return (
      <div className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-medium bg-bg-sidebar border border-border-strong/40 text-text-muted">
        <Edit2 size={11} />
        <span>Locally modified — diverged from source</span>
      </div>
    );
  }

  // "unknown" — render nothing.
  return null;
}

// ── Skill preview — frontmatter header + companion resources + markdown body ──

interface SkillPreviewProps {
  content: string;
  source?: SkillSource;
  sources?: string[];
  resources?: SkillResources | null;
  license?: string;
  /** Called after a successful "Update now" so the parent can re-read the
   * skill content, refresh the library list, and reset the badge state. */
  onUpdated?: (name: string) => void | Promise<void>;
}

function resolveSkillAuthorDescriptor(source?: SkillSource, sources?: string[]): AuthorDescriptor {
  if (source) {
    return source.kind === "bundled"
      ? { type: "provider", name: "Automatic", url: "https://automatic.sh" }
      : { type: "github", repo: source.source };
  }

  if (sources?.includes("codex")) {
    return { type: "provider", name: "OpenAI", url: "https://openai.com" };
  }

  return { type: "local" };
}

function SkillPreview({ content, source, sources, resources, license, onUpdated }: SkillPreviewProps) {
  const { meta, body } = parseFrontmatter(content);
  const displayName = meta.name || "";
  const description = meta.description || "";
  // Prefer the SkillEntry license (already extracted server-side) but fall
  // back to the frontmatter field parsed client-side so the preview is
  // consistent even when the entry is not yet refreshed.
  const displayLicense = license ?? meta.license;

  const hasResources =
    resources && (resources.dirs.length > 0 || resources.root_files.length > 0);

  // Derive AuthorDescriptor from SkillSource (or lack thereof).
  // "bundled" skills are shipped with the app — render as a provider entry
  // rather than doing a GitHub lookup (which would resolve the wrong org).
  const authorDescriptor = resolveSkillAuthorDescriptor(source, sources);

  // Track which directories are expanded
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set());
  const toggleDir = (name: string) =>
    setExpandedDirs(prev => {
      const next = new Set(prev);
      next.has(name) ? next.delete(name) : next.add(name);
      return next;
    });

  // Collapse all when skill changes
  useEffect(() => { setExpandedDirs(new Set()); }, [content]);

  // ── Remote update check ────────────────────────────────────────────────
  // Only GitHub-sourced skills can be checked: bundled skills are shipped
  // with the app, and local skills have no upstream.  The check is best-
  // effort and the badge stays hidden on any error so the preview is not
  // dominated by a network-level concern.
  const [updateStatus, setUpdateStatus] = useState<SkillUpdateStatus | "loading" | null>(null);
  const [updating, setUpdating] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const checkableName = meta.name;
  const canCheckUpdate = source?.kind === "github" && !!checkableName;
  // The Update Now button is shown for every remote-sourced skill so the
  // affordance is discoverable, but it is disabled when no remote source
  // exists.  Per the spec, an up-to-date skill is still updatable — skill
  // versioning is in its infancy, so force-refresh is a legitimate use.
  const canUpdate = canCheckUpdate;

  useEffect(() => {
    if (!canCheckUpdate || !checkableName) {
      setUpdateStatus(null);
      return;
    }
    let cancelled = false;
    setUpdateStatus("loading");
    invoke<SkillUpdateStatus>("check_skill_update", { name: checkableName })
      .then((result) => {
        if (!cancelled) setUpdateStatus(result);
      })
      .catch(() => {
        // Hide the badge on failure — a transient network blip should not
        // look like a permanent "unknown" state.
        if (!cancelled) setUpdateStatus(null);
      });
    return () => {
      cancelled = true;
    };
  }, [canCheckUpdate, checkableName, content]);

  // Reset transient button state when the displayed skill changes so a
  // previous error or in-flight state doesn't leak across selections.
  useEffect(() => {
    setUpdating(false);
    setUpdateError(null);
  }, [checkableName]);

  const handleUpdateNow = async () => {
    if (!canUpdate || !checkableName) return;
    setUpdating(true);
    setUpdateError(null);
    try {
      const status = await invoke<SkillUpdateStatus>("update_skill_from_source", {
        name: checkableName,
      });
      setUpdateStatus(status);
      // Parent owns content/library state — let it refresh so the markdown
      // body and "Used By" sidebar reflect the new bytes.
      await onUpdated?.(checkableName);
    } catch (err: any) {
      setUpdateError(typeof err === "string" ? err : String(err));
    } finally {
      setUpdating(false);
    }
  };

  return (
    <div>
      {/* ── Metadata header ───────────────────────────────────────────── */}
      <div className="px-8 pt-6 pb-0">
        {displayName && (
          <h1 className="text-[20px] font-semibold text-text-base mb-2 leading-tight">{displayName}</h1>
        )}
        {description && (
          <p className="text-[13px] text-text-muted leading-relaxed mb-3">{description}</p>
        )}

        {/* Author section — always shown */}
        <div className={displayLicense ? "mb-2" : "mb-4"}>
          <AuthorSection descriptor={authorDescriptor} />

          {/* Update row: status pill + Update Now action, side by side.
              Always rendered for skills with a recorded source so the
              button is discoverable; disabled when the source has no
              fetchable upstream (e.g. bundled skills). */}
          {source && (
            <div className="mt-2 flex flex-wrap items-center gap-2">
              {canCheckUpdate && updateStatus === "loading" && (
                <span className="text-[11px] text-text-muted/70">Checking for updates…</span>
              )}
              {canCheckUpdate && updateStatus && updateStatus !== "loading" && (
                <SkillUpdateBadge status={updateStatus} />
              )}

              <button
                type="button"
                onClick={handleUpdateNow}
                disabled={!canUpdate || updating}
                title={
                  !canUpdate
                    ? "This skill has no remote source to update from."
                    : "Re-fetch this skill from its source. Safe to run even when up to date."
                }
                className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-medium bg-bg-sidebar border border-border-strong/40 text-text-base hover:bg-bg-input/60 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                <Download size={11} className={updating ? "animate-pulse" : ""} />
                {updating ? "Updating…" : "Update now"}
              </button>
            </div>
          )}

          {updateError && (
            <p className="mt-1.5 text-[11px] text-red-400">
              Update failed: {updateError}
            </p>
          )}
        </div>

        {/* License badge */}
        {displayLicense && (
          <div className="mb-4">
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium bg-bg-sidebar border border-border-strong/40 text-text-muted">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor" className="shrink-0 text-text-muted/70">
                <path d="M8.75.75V2h.985c.304 0 .603.08.867.231l1.29.736c.038.022.08.033.124.033h2.234a.75.75 0 0 1 0 1.5h-.427l2.111 4.692a.75.75 0 0 1-.154.838l-.53-.53.529.531-.001.002-.002.002-.006.006-.006.005-.01.01-.045.04c-.21.176-.441.327-.686.45C14.556 10.78 13.88 11 13 11a4.498 4.498 0 0 1-2.023-.454 3.544 3.544 0 0 1-.686-.45l-.045-.04-.016-.015-.006-.006-.004-.004v-.001a.75.75 0 0 1-.154-.838L12.178 4.5h-.162c-.305 0-.604-.079-.868-.231l-1.29-.736a.245.245 0 0 0-.124-.033H8.75V13h2.5a.75.75 0 0 1 0 1.5h-6.5a.75.75 0 0 1 0-1.5h2.5V3.5h-.984a.245.245 0 0 0-.124.033l-1.29.736c-.264.152-.563.231-.868.231h-.162l2.112 4.692a.75.75 0 0 1-.154.838l-.53-.53.529.531-.001.002-.002.002-.006.006-.016.015-.045.04c-.21.176-.441.327-.686.45C4.556 10.78 3.88 11 3 11a4.498 4.498 0 0 1-2.023-.454 3.544 3.544 0 0 1-.686-.45l-.045-.04-.016-.015-.006-.006-.004-.004v-.001a.75.75 0 0 1-.154-.838L2.178 4.5H1.75a.75.75 0 0 1 0-1.5h2.234a.249.249 0 0 0 .125-.033l1.29-.736c.263-.152.562-.231.866-.231H7.25V.75a.75.75 0 0 1 1.5 0Z"/>
              </svg>
              {displayLicense}
            </span>
          </div>
        )}

        {/* Companion resources */}
        {hasResources && (
          <div className="mb-5 rounded-lg border border-border-strong/40 overflow-hidden">
            <div className="px-3 py-2 bg-bg-sidebar/40 border-b border-border-strong/40">
              <p className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                Additional Resources
              </p>
            </div>

            <div className="divide-y divide-surface">
              {/* Directories — clickable to expand */}
              {resources!.dirs.map(dir => {
                const isOpen = expandedDirs.has(dir.name);
                return (
                  <div key={dir.name}>
                    <button
                      onClick={() => toggleDir(dir.name)}
                      className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-sidebar/60 transition-colors text-left"
                    >
                      {/* Chevron */}
                      <svg
                        width="10" height="10" viewBox="0 0 10 10" fill="currentColor"
                        className={`shrink-0 text-text-muted transition-transform ${isOpen ? "rotate-90" : ""}`}
                      >
                        <path d="M3 2l4 3-4 3V2z"/>
                      </svg>
                      {/* Folder icon */}
                      <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" className="shrink-0 text-brand">
                        <path d="M1.75 1A1.75 1.75 0 0 0 0 2.75v10.5C0 14.216.784 15 1.75 15h12.5A1.75 1.75 0 0 0 16 13.25v-8.5A1.75 1.75 0 0 0 14.25 3H7.5a.25.25 0 0 1-.2-.1l-.9-1.2C6.07 1.26 5.55 1 5 1H1.75z"/>
                      </svg>
                      <span className="text-[12px] font-mono text-text-muted">{dir.name}/</span>
                      <span className="text-[11px] text-text-muted ml-auto">{dir.files.length} {dir.files.length === 1 ? "file" : "files"}</span>
                    </button>

                    {isOpen && (
                      <div className="bg-bg-input/40 border-t border-border-strong/50">
                        {dir.files.map(f => (
                          <div key={f.path} className="flex items-center gap-2 pl-9 pr-3 py-1.5">
                            <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor" className="shrink-0 text-text-muted">
                              <path d="M2 1.75C2 .784 2.784 0 3.75 0h6.586c.464 0 .909.184 1.237.513l2.914 2.914c.329.328.513.773.513 1.237v9.586A1.75 1.75 0 0 1 13.25 16h-9.5A1.75 1.75 0 0 1 2 14.25V1.75z"/>
                            </svg>
                            <span className="text-[12px] font-mono text-text-muted">{f.path}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                );
              })}

              {/* Root-level files */}
              {resources!.root_files.map(f => (
                <div key={f.path} className="flex items-center gap-2 px-3 py-2">
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" className="shrink-0 text-text-muted ml-[22px]">
                    <path d="M2 1.75C2 .784 2.784 0 3.75 0h6.586c.464 0 .909.184 1.237.513l2.914 2.914c.329.328.513.773.513 1.237v9.586A1.75 1.75 0 0 1 13.25 16h-9.5A1.75 1.75 0 0 1 2 14.25V1.75z"/>
                  </svg>
                  <span className="text-[12px] font-mono text-text-muted">{f.path}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="border-b border-border-strong/40 mb-0" />
      </div>

      <MarkdownPreview content={body} />
    </div>
  );
}

// ── Frontmatter field validation ─────────────────────────────────────────────

const XML_TAG_RE = /<[^>]+>/;
const RESERVED_WORDS = ["anthropic", "claude"];
const NAME_CHARSET_RE = /^[a-z0-9-]*$/;

interface FieldError {
  name: string | null;
  description: string | null;
}

function validateSkillName(value: string): string | null {
  if (!value) return "Name is required.";
  if (value.length > 64) return "Name must be 64 characters or fewer.";
  if (!NAME_CHARSET_RE.test(value)) return "Name may only contain lowercase letters, numbers, and hyphens.";
  if (XML_TAG_RE.test(value)) return "Name must not contain XML tags.";
  for (const word of RESERVED_WORDS) {
    if (value === word || value.startsWith(word + "-") || value.endsWith("-" + word) || value.includes("-" + word + "-")) {
      return `Name must not contain the reserved word "${word}".`;
    }
  }
  return null;
}

function validateSkillDescription(value: string): string | null {
  if (!value.trim()) return "Description is required.";
  if (value.length > 1024) return "Description must be 1024 characters or fewer.";
  if (XML_TAG_RE.test(value)) return "Description must not contain XML tags.";
  return null;
}

/** Build the YAML frontmatter block from name + description. */
function buildFrontmatter(name: string, description: string): string {
  // Wrap description in quotes if it contains a colon, to be safe YAML
  const safeDesc = description.includes(":") ? `"${escapeYamlDoubleQuoted(description)}"` : description;
  return `---\nname: ${name}\ndescription: ${safeDesc}\n---\n`;
}


// ── Default template for new skills ──────────────────────────────────────────

/** Body content (no frontmatter) pre-filled when creating a new skill. */
const DEFAULT_SKILL_BODY = `# My Skill

## When to use this skill

Describe the scenarios where this skill should be activated.

## Instructions

Write your skill instructions here. These will be loaded by agents when the skill is active.

### Key behaviors

- Behavior one
- Behavior two
- Behavior three
`;

// ── Collection Tag Bar ────────────────────────────────────────────────────────

interface CollectionTagBarProps {
  skill: string;
  collection: string | null;
  collections: SkillCollection[];
  onChanged: () => void;
}

function CollectionTagBar({ skill, collection, collections, onChanged }: CollectionTagBarProps) {
  const [isAdding, setIsAdding] = useState(false);
  const [inputValue, setInputValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isAdding) inputRef.current?.focus();
  }, [isAdding]);

  // Reset adding state when skill changes
  useEffect(() => { setIsAdding(false); setInputValue(""); }, [skill]);

  const handleAssign = async (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      await invoke("set_skill_collection", { skillName: skill, collection: trimmed });
      onChanged();
    } catch (err) {
      console.error("Failed to set collection:", err);
    }
    setIsAdding(false);
    setInputValue("");
  };

  const handleRemove = async () => {
    try {
      await invoke("remove_skill_collection", { skillName: skill });
      onChanged();
    } catch (err) {
      console.error("Failed to remove collection:", err);
    }
  };

  const collectionNames = collections.map(c => c.name);

  return (
    <div className="h-9 px-5 border-b border-border-strong/40 flex items-center gap-2 shrink-0 bg-bg-input/30">
      <Tag size={11} className="text-text-muted shrink-0" />
      <span className="text-[10px] font-semibold text-text-muted tracking-wider uppercase shrink-0">
        Collection
      </span>

      {collection ? (
        <span className="flex items-center gap-1 px-2 py-0.5 rounded-full bg-icon-file-template/10 border border-icon-file-template/20 text-[11px] font-medium text-icon-file-template">
          {collection}
          <button
            onClick={handleRemove}
            className="ml-0.5 hover:text-danger transition-colors"
            title="Remove from collection"
          >
            <X size={10} />
          </button>
        </span>
      ) : isAdding ? (
        <div className="flex items-center gap-1">
          <input
            ref={inputRef}
            type="text"
            list="collection-suggestions"
            value={inputValue}
            onChange={e => setInputValue(e.target.value)}
            onKeyDown={e => {
              if (e.key === "Enter") handleAssign(inputValue);
              if (e.key === "Escape") { setIsAdding(false); setInputValue(""); }
            }}
            onBlur={() => {
              if (inputValue.trim()) handleAssign(inputValue);
              else { setIsAdding(false); setInputValue(""); }
            }}
            placeholder="Collection name…"
            className="w-40 px-2 py-0.5 rounded bg-bg-sidebar border border-border-strong/40 focus:border-brand outline-none text-[11px] text-text-base placeholder-text-muted/60"
          />
          <datalist id="collection-suggestions">
            {collectionNames.map(name => (
              <option key={name} value={name} />
            ))}
          </datalist>
        </div>
      ) : (
        <button
          onClick={() => setIsAdding(true)}
          className="flex items-center gap-1 px-2 py-0.5 rounded text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar border border-dashed border-border-strong/40 transition-colors"
        >
          <Plus size={10} />
          Add to collection
        </button>
      )}
    </div>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

interface SkillsProps {
  /** Pre-select this skill when the component mounts / when it changes. */
  initialSkill?: string | null;
  /** Called once the initial skill has been applied so the parent can clear it. */
  onInitialSkillConsumed?: () => void;
  /** Navigate to the Projects tab, pre-selecting the given project. */
  onNavigateToProject?: (name: string) => void;
  /** Navigate to the Project Templates tab, pre-selecting the given template. */
  onNavigateToTemplate?: (name: string) => void;
}

export default function Skills({ initialSkill = null, onInitialSkillConsumed, onNavigateToProject, onNavigateToTemplate }: SkillsProps = {}) {
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("skills", recentRefresh);
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [newSkillName, setNewSkillName] = useState("");
  const [newSkillDescription, setNewSkillDescription] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [filter, setFilter] = useState<"all" | "remote" | "local">("all");
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [securityNotice, setSecurityNotice] = useState<string | null>(null);
  const [currentScan, setCurrentScan] = useState<AssetSecurityScanRecord | null>(null);
  const [collectionFilter, setCollectionFilter] = useState<string | null>(null);
  const [collections, setCollections] = useState<SkillCollection[]>([]);

  const [showImportDialog, setShowImportDialog] = useState(false);
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkProgress, setBulkProgress] = useState<{ done: number; total: number } | null>(null);

  // Companion resources for the selected skill
  const [skillResources, setSkillResources] = useState<SkillResources | null>(null);

  // Projects and templates that reference the selected skill
  const [skillUsedBy, setSkillUsedBy] = useState<SkillUsedBy | null>(null);

  // Frontmatter field state for the edit panel (existing skills)
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [fieldErrors, setFieldErrors] = useState<FieldError>({ name: null, description: null });
  // Body content without frontmatter, for the split editor
  const [editBody, setEditBody] = useState("");

  useEffect(() => { loadSkills(); }, []);

  // Navigate to the skill specified by the parent (e.g. "View in library" from Projects)
  useEffect(() => {
    if (!initialSkill) return;
    // Wait until skills are loaded, then select the requested one
    if (skills.length === 0) return;
    const exists = skills.some((s) => s.name === initialSkill);
    if (exists) {
      loadSkillContent(initialSkill);
    }
    onInitialSkillConsumed?.();
  }, [initialSkill, skills]);

  // ── Data ──────────────────────────────────────────────────────────────────

  const loadSkills = async () => {
    try {
      const [result, cols] = await Promise.all([
        invoke<SkillEntry[]>("get_skills"),
        invoke<SkillCollection[]>("get_skill_collections"),
      ]);
      setSkills(result.sort((a, b) => a.name.localeCompare(b.name)));
      setCollections(cols);
      setError(null);
    } catch (err: any) {
      setError(`Failed to load skills: ${err}`);
    }
  };

  const loadSkillContent = async (name: string) => {
    try {
      const [content, resources, projectNames, templateNames] = await Promise.all([
        invoke<string>("read_skill", { name }),
        invoke<SkillResources>("get_skill_resources", { name }),
        invoke<string[]>("get_projects"),
        invoke<string[]>("get_templates"),
      ]);
      const scan = await scanAssetContent("skill", content);
      setSelectedSkill(name);
      setSkillContent(content);
      setSkillResources(resources);

      // Resolve which projects and templates reference this skill.
      // Both commands return a raw JSON string (double-encoded by Tauri), so we parse manually.
      const [projectDetails, templateDetails] = await Promise.all([
        Promise.all(projectNames.map(n =>
          invoke<string>("read_project", { name: n })
            .then(raw => { const p = JSON.parse(raw); return { name: n, skills: Array.isArray(p.skills) ? p.skills as string[] : [] }; })
            .catch(() => null)
        )),
        Promise.all(templateNames.map(n =>
          invoke<string>("read_template", { name: n })
            .then(raw => { const t = JSON.parse(raw); return { name: n, skills: Array.isArray(t.skills) ? t.skills as string[] : [] }; })
            .catch(() => null)
        )),
      ]);

      const usingProjects = projectDetails.filter(p => p && p.skills.includes(name)).map(p => p!.name);
      const usingTemplates = templateDetails.filter(t => t && t.skills.includes(name)).map(t => t!.name);
      setSkillUsedBy({ projects: usingProjects, templates: usingTemplates });

      // Parse frontmatter into edit fields
      const { meta, body } = parseFrontmatter(content);
      setEditName(meta.name ?? name);
      setEditDescription(meta.description ?? "");
      setEditBody(body);
      setFieldErrors({ name: null, description: null });
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
      setCurrentScan(toAssetSecurityScanRecord(scan));
      setSecurityNotice(
        scan.findings.length > 0
          ? formatAssetScanResult(scan, "skill", {
              blockedHeader: "Dangerous content found in skill:",
            })
          : null,
      );
    } catch (err: any) {
      setError(`Failed to read skill ${name}: ${err}`);
    }
  };

  const handleSave = async () => {
    if (isCreating) {
      // Validate create form fields
      const nameErr = validateSkillName(newSkillName);
      const descErr = validateSkillDescription(newSkillDescription);
      setFieldErrors({ name: nameErr, description: descErr });
      if (nameErr || descErr) return;

      const content = buildFrontmatter(newSkillName, newSkillDescription) + "\n" + editBody;
      try {
        const scan = await scanAssetContent("skill", content);
        if (scan.blocked) {
          setError(formatAssetScanResult(scan, "skill"));
          setSecurityNotice(null);
          return;
        }
        const warnings = warningFindings(scan);
        await invoke("save_skill", { name: newSkillName, content });
        trackSkillCreated(newSkillName, "local");
        setIsCreating(false);
        setIsEditing(false);
        setCurrentScan(toAssetSecurityScanRecord(scan));
        await loadSkills();
        await loadSkillContent(newSkillName);
        setError(null);
        setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "skill") : null);
        setRecentRefresh(prev => prev + 1);
      } catch (err: any) {
        setError(`Failed to save skill: ${err}`);
      }
    } else {
      // Validate edit form fields
      const nameErr = validateSkillName(editName);
      const descErr = validateSkillDescription(editDescription);
      setFieldErrors({ name: nameErr, description: descErr });
      if (nameErr || descErr) return;

      const finalContent = buildFrontmatter(editName, editDescription) + "\n" + editBody;
      try {
        const scan = await scanAssetContent("skill", finalContent);
        if (scan.blocked) {
          setError(formatAssetScanResult(scan, "skill"));
          setSecurityNotice(null);
          return;
        }
        const warnings = warningFindings(scan);
        await invoke("save_skill", { name: selectedSkill!, content: finalContent });
        trackSkillUpdated(selectedSkill!);
        setSkillContent(finalContent);
        setIsEditing(false);
        setCurrentScan(toAssetSecurityScanRecord(scan));
        setError(null);
        setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "skill") : null);
      } catch (err: any) {
        setError(`Failed to save skill: ${err}`);
      }
    }
  };

  // ── Selection ─────────────────────────────────────────────────────────────
  //
  // A skill is deletable when it is not the built-in `automatic` skill, not a
  // bundled skill shipped with the app (those get reinstalled on next launch,
  // so deleting them looks like the delete was ignored), and not provided by
  // a plugin. The bulk toolbar and per-row checkbox mirror this rule so
  // undeletable skills never end up in the selection, and the backend refuses
  // the same set defensively.

  const isDeletable = (skill: SkillEntry) =>
    skill.name !== "automatic"
    && !skill.plugin_id
    && skill.source?.kind !== "bundled";

  const handleBulkDelete = async () => {
    // Only delete what is both selected AND currently deletable — a race where
    // the list changes underneath the selection must not delete `automatic`
    // or a plugin skill by mistake.
    const targets = skills.filter(s => selection.selectedIds.has(s.name) && isDeletable(s));
    if (targets.length === 0) return;

    const preview = targets.slice(0, 10).map(t => `• ${t.name}`).join("\n");
    const overflow = targets.length > 10 ? `\n…and ${targets.length - 10} more.` : "";
    const message = `Delete ${targets.length} skill${targets.length === 1 ? "" : "s"}?\n\n${preview}${overflow}\n\nSkill directories will be removed from disk. This cannot be undone.`;
    const confirmed = await ask(message, { title: "Delete Skills", kind: "warning" });
    if (!confirmed) return;

    setBulkDeleting(true);
    setBulkProgress({ done: 0, total: targets.length });
    const failed: { name: string; error: string }[] = [];
    for (let i = 0; i < targets.length; i++) {
      const name = targets[i]!.name;
      try {
        await invoke("delete_skill", { name });
        trackSkillDeleted(name);
      } catch (err: any) {
        failed.push({ name, error: String(err) });
      }
      setBulkProgress({ done: i + 1, total: targets.length });
    }

    // Close the drawer if its skill was in the deleted set.
    if (selectedSkill && targets.some(t => t.name === selectedSkill)) {
      setSelectedSkill(null);
      setSkillContent("");
      setSkillUsedBy(null);
      setIsEditing(false);
      setCurrentScan(null);
    }

    await loadSkills();
    selection.clearSelection();
    setBulkDeleting(false);
    setBulkProgress(null);
    if (failed.length > 0) {
      const detail = failed.slice(0, 5).map(f => `${f.name}: ${f.error}`).join("\n");
      const more = failed.length > 5 ? `\n…and ${failed.length - 5} more.` : "";
      setError(`Failed to delete ${failed.length} skill${failed.length === 1 ? "" : "s"}:\n${detail}${more}`);
    } else {
      setError(null);
    }
  };

  const handleDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const target = skills.find(s => s.name === name);
    if (target && !isDeletable(target)) return;
    const externalSources = (target?.sources ?? []).filter(src => src !== "library");
    let message = `Delete skill "${name}"?`;
    if (externalSources.length > 0) {
      try {
        const dirs = await invoke<{ id: string; path: string }[]>("list_skill_directories");
        const paths = externalSources
          .map(src => dirs.find(d => d.id === src)?.path)
          .filter((p): p is string => !!p)
          .map(p => `${p}/${name}`);
        if (paths.length > 0) {
          message = `Delete skill "${name}"?\n\nThis will remove the following directories from disk:\n\n${paths.join("\n")}`;
        }
      } catch {
        // fall back to the simple confirmation
      }
    }
    const confirmed = await ask(message, { title: "Delete Skill", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_skill", { name });
      trackSkillDeleted(name);
      if (selectedSkill === name) { setSelectedSkill(null); setSkillContent(""); setSkillUsedBy(null); setIsEditing(false); setCurrentScan(null); }
      await loadSkills();
      setError(null);
      setSecurityNotice(null);
    } catch (err: any) {
      setError(`Failed to delete skill: ${err}`);
    }
  };



  const handleImportToLibrary = async (name: string) => {
    try {
      await invoke("sync_skill", { name });
      await loadSkills();
      await loadSkillContent(name);
      setError(null);
      setSecurityNotice(null);
    } catch (err: any) {
      setError(`Failed to import skill to library: ${err}`);
    }
  };

  const handleDuplicate = async (name: string) => {
    // Build a unique local name: "<name>-local", or "<name>-local-2", etc.
    const base = `${name}-local`;
    let candidate = base;
    let suffix = 2;
    while (skills.some(s => s.name === candidate)) {
      candidate = `${base}-${suffix}`;
      suffix++;
    }
    try {
      const raw = await invoke<string>("read_skill", { name });
      // Strip the source from the frontmatter by rebuilding it from parsed fields.
      const { meta, body } = parseFrontmatter(raw);
      const dupName = candidate;
      const dupDesc = meta.description ?? "";
      const content = buildFrontmatter(dupName, dupDesc) + "\n" + body;
      await invoke("save_skill", { name: dupName, content });
      trackSkillCreated(dupName, "local");
      await loadSkills();
      await loadSkillContent(dupName);
      setError(null);
      setSecurityNotice(null);
    } catch (err: any) {
      setError(`Failed to duplicate skill: ${err}`);
    }
  };

  const startCreateNew = () => {
    setSelectedSkill(null);
    setNewSkillName("");
    setNewSkillDescription("");
    setEditBody(DEFAULT_SKILL_BODY);
    setSkillContent("");
    setSkillResources(null);
    setSkillUsedBy(null);
    setCurrentScan(null);
    setFieldErrors({ name: null, description: null });
    setIsCreating(true);
    setIsEditing(true);
    setSecurityNotice(null);
  };

  // ── Derived ───────────────────────────────────────────────────────────────

  const remoteCount = skills.filter(s => !!s.source).length;
  const localCount = skills.length - remoteCount;

  const searchLower = search.trim().toLowerCase();
  const filteredSkills = skills.filter(s => {
    if (filter === "remote" && !s.source) return false;
    if (filter === "local" && !!s.source) return false;
    if (collectionFilter && s.collection !== collectionFilter) return false;
    if (searchLower && !s.name.toLowerCase().includes(searchLower)) return false;
    return true;
  });

  const selectedEntry = skills.find(s => s.name === selectedSkill);
  const { label: scanStatusLabel, className: scanStatusClass } = getAssetSecurityStatus(currentScan, {
    blockedLabel: "Danger",
  });
  const scanTimestamp = currentScan
    ? new Date(currentScan.scanned_at).toLocaleString()
    : null;
  const securityNoticeToneClass = getAssetSecurityNoticeClass(currentScan);
  const securityDismissButtonClass = getAssetSecurityDismissButtonClass(currentScan);

  // ── Selection derived ─────────────────────────────────────────────────────
  // The header checkbox only toggles rows the user is allowed to delete —
  // undeletable rows never receive a checkbox, so their state is irrelevant.
  const selection = useBulkSelection(filteredSkills, s => s.name, isDeletable);
  const drawerOpen = isCreating || !!selectedSkill;

  const closeDrawer = () => {
    setIsCreating(false);
    setIsEditing(false);
    setSelectedSkill(null);
    setSkillContent("");
    setSkillUsedBy(null);
    setNewSkillName("");
    setNewSkillDescription("");
    setFieldErrors({ name: null, description: null });
    setCurrentScan(null);
    setSecurityNotice(null);
  };

  const originLabel = (skill: SkillEntry): { label: string; className: string; title?: string } => {
    if (skill.source) {
      if (skill.source.kind === "bundled") {
        return { label: "Bundled", className: "text-text-muted", title: "Shipped with Automatic" };
      }
      return { label: skill.source.source, className: "text-success", title: `Installed from ${skill.source.source}` };
    }
    if (skill.plugin_id) {
      return { label: skill.plugin_id, className: "text-text-muted", title: `Provided by plugin ${skill.plugin_id}` };
    }
    return { label: "Local", className: "text-text-muted", title: "Created locally" };
  };

  const renderTableRow = (skill: SkillEntry) => {
    const isRowSelected = selection.isSelected(skill.name);
    const isFocused = selectedSkill === skill.name && !isCreating;
    const deletable = isDeletable(skill);
    const isRemote = !!skill.source;
    const isExternalOnly = skill.sources.length > 0 && !skill.sources.includes("library");
    const origin = originLabel(skill);
    return (
      <tr
        key={skill.name}
        onClick={() => loadSkillContent(skill.name)}
        className={`group cursor-pointer border-b border-border-strong/20 last:border-b-0 transition-colors ${
          isFocused ? "bg-bg-sidebar/60" : "hover:bg-bg-input/70"
        }`}
      >
        <td className="px-3 py-2 w-9" onClick={(e) => e.stopPropagation()}>
          {deletable ? (
            <input
              type="checkbox"
              checked={isRowSelected}
              onChange={() => selection.toggleSelected(skill.name)}
              aria-label={`Select ${skill.name}`}
              className="cursor-pointer accent-brand"
            />
          ) : (
            <LockCell
              tooltip={
                skill.name === "automatic"
                  ? "Built-in skill — cannot be deleted"
                  : skill.source?.kind === "bundled"
                    ? "Bundled with Automatic — cannot be deleted"
                    : `Plugin-provided (${skill.plugin_id}) — cannot be deleted`
              }
            />
          )}
        </td>
        <td className="px-3 py-2 w-11">
          <SkillAvatar name={skill.name} source={skill.source?.source} kind={skill.source?.kind} size={28} />
        </td>
        <td className="px-3 py-2 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-[13px] font-medium text-text-base truncate">{skill.name}</span>
            {recentIds.has(skill.name) && (
              <span className="shrink-0 px-1.5 py-0.5 rounded bg-brand/15 text-brand text-[9px] font-semibold uppercase tracking-wider">New</span>
            )}
            {isExternalOnly && (
              <span
                className="shrink-0 px-1.5 py-0.5 rounded bg-warning/10 text-[9px] text-warning"
                title="Skill lives outside Automatic's managed library. Import to sync it into projects."
              >
                External
              </span>
            )}
            {skill.has_resources && (
              <span title="Has additional resources" className="shrink-0 text-text-muted">
                <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M1.75 1A1.75 1.75 0 0 0 0 2.75v10.5C0 14.216.784 15 1.75 15h12.5A1.75 1.75 0 0 0 16 13.25v-8.5A1.75 1.75 0 0 0 14.25 3H7.5a.25.25 0 0 1-.2-.1l-.9-1.2C6.07 1.26 5.55 1 5 1H1.75z"/>
                </svg>
              </span>
            )}
          </div>
        </td>
        <td className="px-3 py-2">
          <span
            className={`inline-flex items-center gap-1 text-[11px] ${origin.className} truncate max-w-[200px]`}
            title={origin.title}
          >
            {isRemote && skill.source!.kind !== "bundled" && <Globe size={10} className="shrink-0" />}
            {skill.plugin_id && <Puzzle size={10} className="shrink-0" />}
            <span className="truncate">{origin.label}</span>
          </span>
        </td>
        <td className="px-3 py-2">
          {skill.sources && skill.sources.length > 0 && (
            <span className="inline-flex flex-wrap items-center gap-1">
              {skill.sources.map(src => {
                const label = src === "library"
                  ? "library"
                  : src === "agents"
                    ? "~/.agents"
                    : src === "claude"
                      ? "~/.claude"
                      : src;
                return (
                  <button
                    key={src}
                    onClick={async (e) => {
                      e.stopPropagation();
                      try {
                        const sources = await invoke<{ id: string; path: string }[]>("list_skill_directories");
                        const source = sources.find(s => s.id === src);
                        if (source) {
                          const skillDir = `${source.path}/${skill.name}`;
                          await openPath(skillDir);
                        }
                      } catch (err) {
                        console.error("Failed to open skill directory:", err);
                      }
                    }}
                    className="px-1.5 py-0.5 rounded bg-bg-base/50 text-[10px] text-text-muted hover:bg-brand/20 hover:text-brand transition-colors"
                    title={`Open ${label} skill folder`}
                  >
                    {label}
                  </button>
                );
              })}
            </span>
          )}
        </td>
        <td className="px-3 py-2">
          {skill.collection ? (
            <button
              onClick={(e) => {
                e.stopPropagation();
                setCollectionFilter(collectionFilter === skill.collection ? null : skill.collection!);
              }}
              className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-icon-file-template/10 text-[10px] text-icon-file-template hover:bg-icon-file-template/20 transition-colors"
              title={`Filter by collection: ${skill.collection}`}
            >
              <Tag size={9} />
              <span className="truncate max-w-[120px]">{skill.collection}</span>
            </button>
          ) : (
            <span className="text-[11px] text-text-muted/50">—</span>
          )}
        </td>
        <td className="px-3 py-2 w-16 text-right" onClick={(e) => e.stopPropagation()}>
          {deletable ? (
            <button
              onClick={(e) => handleDelete(skill.name, e)}
              className="opacity-0 group-hover:opacity-100 p-1 text-text-muted hover:text-danger rounded transition-all"
              title="Delete skill"
            >
              <X size={13} />
            </button>
          ) : null}
        </td>
      </tr>
    );
  };

  return (
    <div className="flex h-full w-full flex-col bg-bg-base">

      {/* ── Top Toolbar ──────────────────────────────────────────────────── */}
      <div className="shrink-0 border-b border-border-strong/40 bg-bg-input/40">
        <div className="flex items-center justify-between px-4 pt-3 pb-2 gap-3">
          <div className="flex items-center gap-3 min-w-0">
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
              Skills
            </span>
            <div className="flex items-center gap-1 bg-bg-base rounded-md border border-border-strong/40 p-0.5">
              {(["all", "remote", "local"] as const).map(f => (
                <button
                  key={f}
                  onClick={() => setFilter(f)}
                  className={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors ${
                    filter === f
                      ? "bg-bg-sidebar text-text-base"
                      : "text-text-muted hover:text-text-base"
                  }`}
                >
                  {f === "all" ? `All ${skills.length}` : f === "remote" ? `Remote ${remoteCount}` : `Local ${localCount}`}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              <input
                type="text"
                placeholder="Search skills…"
                value={search}
                onChange={e => setSearch(e.target.value)}
                className="w-56 h-7 pl-7 pr-7 rounded-md bg-bg-input border border-border-strong/50 hover:border-border-strong focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 text-[12px] text-text-base placeholder-text-muted/60 transition-colors"
              />
              {search && (
                <button
                  onClick={() => setSearch("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={11} />
                </button>
              )}
            </div>

            {collections.length > 0 && (
              <div className="relative">
                <Tag size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
                <select
                  value={collectionFilter ?? ""}
                  onChange={e => setCollectionFilter(e.target.value || null)}
                  aria-label="Filter by collection"
                  className="h-7 min-w-[170px] appearance-none rounded-md border border-border-strong/50 bg-bg-input pl-7 pr-7 text-[12px] text-text-base focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
                >
                  <option value="">All collections</option>
                  {collections.map(c => (
                    <option key={c.name} value={c.name}>
                      {c.name} ({c.skills.length})
                    </option>
                  ))}
                </select>
                <ChevronDown size={11} className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              </div>
            )}

            <button
              onClick={() => setShowImportDialog(true)}
              className="flex items-center gap-1.5 h-7 px-2.5 rounded-md border border-border-strong/50 bg-bg-input hover:bg-bg-sidebar text-[12px] text-text-base transition-colors"
              title="Import Skill"
            >
              <Download size={12} /> Import
            </button>

            <button
              onClick={startCreateNew}
              className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-brand hover:bg-brand-hover text-white text-[12px] font-medium transition-colors"
              title="New Skill"
            >
              <Plus size={12} /> New Skill
            </button>
          </div>
        </div>

        {/* Selection action bar — appears whenever anything is selected */}
        {selection.totalSelected > 0 && (
          <div className="flex items-center justify-between px-4 py-2 border-t border-border-strong/30 bg-brand/5">
            <span className="text-[12px] text-text-base">
              {selection.totalSelected} skill{selection.totalSelected === 1 ? "" : "s"} selected
              {bulkProgress && (
                <span className="ml-2 text-text-muted">
                  · Deleting {bulkProgress.done}/{bulkProgress.total}…
                </span>
              )}
            </span>
            <div className="flex items-center gap-2">
              <button
                onClick={selection.clearSelection}
                disabled={bulkDeleting}
                className="h-7 px-2.5 rounded-md text-[12px] text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors disabled:opacity-50"
              >
                Clear selection
              </button>
              <button
                onClick={handleBulkDelete}
                disabled={bulkDeleting}
                className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-danger/90 hover:bg-danger text-white text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-wait"
              >
                <X size={12} /> Delete selected
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Error + security banners */}
      {error && (
        <div className="border-b border-red-300/80 bg-red-50 p-3 text-[13px] text-red-950 flex items-center justify-between shrink-0">
          <div className="whitespace-pre-wrap">{error}</div>
          <button
            onClick={() => setError(null)}
            className="text-red-900/70 hover:text-red-950 transition-colors"
          >
            <X size={14} />
          </button>
        </div>
      )}
      {securityNotice && (
        <div className={`${securityNoticeToneClass} p-3 text-[13px] border-b flex items-center justify-between shrink-0`}>
          <div className="whitespace-pre-wrap">{securityNotice}</div>
          <button
            onClick={() => setSecurityNotice(null)}
            className={securityDismissButtonClass}
          >
            <X size={14} />
          </button>
        </div>
      )}

      {/* ── Table ────────────────────────────────────────────────────────── */}
      <AssetTable
        items={filteredSkills}
        getId={s => s.name}
        isEmpty={skills.length === 0}
        emptyState={
          <>
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-icon-skill/12 border border-icon-skill/20 flex items-center justify-center">
              <Code size={22} className={ICONS.skill.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">No skills yet</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              Skills are reusable instruction sets that agents load on demand. Create your first skill or import one to get started.
            </p>
            <div className="flex items-center gap-3">
              <button
                onClick={startCreateNew}
                className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
              >
                <Plus size={14} /> New Skill
              </button>
              <button
                onClick={() => setShowImportDialog(true)}
                className="flex items-center gap-2 px-4 py-2 bg-bg-sidebar hover:bg-surface-hover border border-border-strong text-text-base rounded-lg text-[13px] font-medium transition-colors"
              >
                <Download size={14} /> Import
              </button>
            </div>
          </>
        }
        noMatchState={
          <p className="text-[13px] text-text-muted">
            {searchLower ? `No skills match "${search}".` : "No skills match the current filter."}
          </p>
        }
        columns={[
          { key: "avatar", header: "", className: "w-11" },
          { key: "name", header: "Name" },
          { key: "origin", header: "Origin" },
          { key: "locations", header: "Locations" },
          { key: "collection", header: "Collection" },
          { key: "actions", header: "", className: "w-16" },
        ]}
        renderRow={renderTableRow}
        selection={{
          allSelected: selection.allSelected,
          someSelected: selection.someSelected,
          disabled: selection.deletableItems.length === 0,
          onToggleAll: selection.toggleSelectAllVisible,
          ariaLabel: "Select all visible deletable skills",
        }}
        recentIds={recentIds}
      />

      {/* ── Drawer ───────────────────────────────────────────────────────── */}
      <AssetDrawer open={drawerOpen} onClose={closeDrawer} isEditing={isEditing}>
        {isCreating ? (
          /* ── New Skill Form ─────────────────────────────────────────────── */
          <div className="flex-1 flex flex-col h-full min-h-0">

            {/* Header */}
            <div className="h-11 pl-5 pr-10 border-b border-border-strong/40 flex items-center justify-between shrink-0">
              <div className="flex items-center gap-2 min-w-0">
                <Plus size={13} className={`${ICONS.skill.iconColor} shrink-0`} />
                <span className="text-[14px] font-medium text-text-base">New Skill</span>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <button
                  onClick={() => {
                    setIsCreating(false);
                    setIsEditing(false);
                    setSelectedSkill(null);
                    setSkillContent("");
                    setNewSkillName("");
                    setNewSkillDescription("");
                    setFieldErrors({ name: null, description: null });
                  }}
                  className="px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={handleSave}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors"
                >
                  <Check size={12} /> Create Skill
                </button>
              </div>
            </div>

            {/* Frontmatter fields */}
            <div className="px-6 pt-5 pb-4 border-b border-border-strong/40 shrink-0 space-y-4">

              {/* name */}
              <div>
                <div className="flex items-baseline justify-between mb-1.5">
                  <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                    Name <span className="text-red-400 ml-0.5">*</span>
                  </label>
                  <span className={`text-[11px] tabular-nums ${newSkillName.length > 58 ? (newSkillName.length > 64 ? "text-red-400" : "text-warning") : "text-text-muted"}`}>
                    {newSkillName.length}/64
                  </span>
                </div>
                <input
                  type="text"
                  placeholder="my-skill-name"
                  value={newSkillName}
                  onChange={(e) => {
                    const raw = e.target.value.toLowerCase().replace(/\s+/g, "-").replace(/[^a-z0-9-]/g, "");
                    setNewSkillName(raw);
                    setSelectedSkill(raw || null);
                    setFieldErrors(prev => ({ ...prev, name: validateSkillName(raw) }));
                  }}
                  autoFocus
                  maxLength={64}
                  className={`w-full px-3 py-2 rounded-md bg-bg-sidebar border outline-none text-[13px] text-text-base placeholder-text-muted/40 font-mono transition-colors ${
                    fieldErrors.name ? "border-red-500/60 focus:border-red-500" : "border-border-strong/40 hover:border-border-strong focus:border-brand"
                  }`}
                  spellCheck={false}
                />
                {fieldErrors.name ? (
                  <p className="mt-1.5 text-[11px] text-red-400">{fieldErrors.name}</p>
                ) : (
                  <p className="mt-1.5 text-[11px] text-text-muted">
                    Lowercase letters, digits, and hyphens only. Becomes the directory name in Automatic's managed library at <code className="font-mono">~/.automatic/library/skills/</code>.
                  </p>
                )}
              </div>

              {/* description */}
              <div>
                <div className="flex items-baseline justify-between mb-1.5">
                  <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                    Description <span className="text-red-400 ml-0.5">*</span>
                  </label>
                  <span className={`text-[11px] tabular-nums ${newSkillDescription.length > 900 ? (newSkillDescription.length > 1024 ? "text-red-400" : "text-warning") : "text-text-muted"}`}>
                    {newSkillDescription.length}/1024
                  </span>
                </div>
                <textarea
                  placeholder="A concise description of what this skill does and when to use it."
                  value={newSkillDescription}
                  onChange={(e) => {
                    setNewSkillDescription(e.target.value);
                    setFieldErrors(prev => ({ ...prev, description: validateSkillDescription(e.target.value) }));
                  }}
                  rows={3}
                  maxLength={1024}
                  className={`w-full px-3 py-2 rounded-md bg-bg-sidebar border outline-none text-[13px] text-text-base placeholder-text-muted/40 resize-none transition-colors leading-relaxed ${
                    fieldErrors.description ? "border-red-500/60 focus:border-red-500" : "border-border-strong/40 hover:border-border-strong focus:border-brand"
                  }`}
                  spellCheck={false}
                />
                {fieldErrors.description && (
                  <p className="mt-1 text-[11px] text-red-400">{fieldErrors.description}</p>
                )}
              </div>
            </div>

            {/* Body editor */}
            <div className="flex-1 min-h-0 flex flex-col">
              <div className="px-6 pt-3 pb-2 shrink-0">
                <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                  Body
                </label>
              </div>
              <LineNumberedTextarea
                value={editBody}
                onChange={setEditBody}
                className="flex-1"
              />
            </div>
          </div>

        ) : selectedSkill ? (
          /* ── Existing Skill View/Edit ────────────────────────────────────── */
          <div className="flex-1 flex flex-col h-full min-h-0">

            {/* Header */}
            <div className="h-11 pl-5 pr-10 border-b border-border-strong/40 flex items-center justify-between shrink-0">
              <div className="flex items-center gap-2.5 min-w-0">
                <FileText size={13} className={`${ICONS.skill.iconColor} shrink-0`} />
                <>
                  <h3 className="text-[14px] font-medium text-text-base truncate">{selectedSkill}</h3>
                  {selectedEntry?.source && (
                    <>
                      <span className="text-surface">/</span>
                      <a
                        href={`https://github.com/${selectedEntry.source.source}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        onClick={handleExternalLinkClick(`https://github.com/${selectedEntry.source.source}`)}
                        className="flex items-center gap-1 text-[11px] text-text-muted hover:text-text-base transition-colors truncate"
                      >
                        <Github size={11} />
                        {selectedEntry.source.source}
                      </a>
                    </>
                  )}
                </>
              </div>

              <div className="flex items-center gap-2 shrink-0">
                <TokenPill text={isEditing ? editBody : skillContent} />
                {/* Built-in badge for the automatic skill */}
                {selectedSkill === "automatic" && <BuiltInBadge />}
                {/* External badge — skill lives outside the managed library */}
                {selectedEntry && selectedEntry.sources.length > 0 && !selectedEntry.sources.includes("library") && !isEditing && (
                  <span className="flex items-center gap-1 px-2 py-1 rounded text-[11px] text-warning bg-warning/10 border border-warning/20" title="This skill lives in ~/.agents/skills or a similar external location. Import it to Automatic's library to sync it into projects.">
                    <span>External</span>
                  </span>
                )}
                {/* Remote skills are read-only */}
                {selectedEntry?.source && !isEditing && selectedSkill !== "automatic" && (
                  <ReadOnlyBadge tooltip="Installed from a remote source — editing is disabled" />
                )}
                {/* Import to library — offered for external-only skills */}
                {!isEditing && selectedEntry && selectedEntry.sources.length > 0 && !selectedEntry.sources.includes("library") && (
                  <button
                    onClick={() => handleImportToLibrary(selectedSkill!)}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors"
                    title="Copy this skill into Automatic's managed library so it can be synced to projects"
                  >
                    <Download size={12} /> Import to library
                  </button>
                )}
                {!isEditing && (
                  <button
                    onClick={() => handleDuplicate(selectedSkill!)}
                    className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                    title="Duplicate as a local, editable copy"
                  >
                    <Copy size={12} /> Duplicate
                  </button>
                )}
                {!isEditing && !selectedEntry?.source && (
                  <button
                    onClick={() => setIsEditing(true)}
                    className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                  >
                    <Edit2 size={12} /> Edit
                  </button>
                )}
                {isEditing && (
                  <>
                    <button
                      onClick={() => { setIsEditing(false); loadSkillContent(selectedSkill!); }}
                      className="px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={handleSave}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors"
                    >
                      <Check size={12} /> Save
                    </button>
                  </>
                )}
              </div>
            </div>

            {/* Collection tag bar */}
            {!isEditing && (
              <CollectionTagBar
                skill={selectedSkill}
                collection={selectedEntry?.collection ?? null}
                collections={collections}
                onChanged={loadSkills}
              />
            )}

            {!isEditing && (
              <div className="px-5 py-2.5 border-b border-border-strong/40 flex items-center gap-2 shrink-0 bg-bg-input/20">
                <span className="text-[10px] font-semibold text-text-muted tracking-wider uppercase">
                  Current Security Scan
                </span>
                <span className={`px-2 py-0.5 rounded-full border text-[11px] font-medium ${scanStatusClass}`}>
                  {scanStatusLabel}
                </span>
                <span className="text-[11px] text-text-muted">
                  {scanTimestamp ? scanTimestamp : "No recorded scan yet"}
                </span>
              </div>
            )}

            {/* Body */}
            <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
              {isEditing ? (
                <>
                  {/* Frontmatter fields */}
                  <div className="px-6 pt-5 pb-4 border-b border-border-strong/40 shrink-0 space-y-4">

                    {/* name */}
                    <div>
                      <div className="flex items-baseline justify-between mb-1.5">
                        <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                          Name <span className="text-red-400 ml-0.5">*</span>
                        </label>
                        <span className={`text-[11px] tabular-nums ${editName.length > 58 ? (editName.length > 64 ? "text-red-400" : "text-warning") : "text-text-muted"}`}>
                          {editName.length}/64
                        </span>
                      </div>
                      <input
                        type="text"
                        placeholder="my-skill-name"
                        value={editName}
                        onChange={(e) => {
                          const raw = e.target.value.toLowerCase().replace(/\s+/g, "-").replace(/[^a-z0-9-]/g, "");
                          setEditName(raw);
                          setFieldErrors(prev => ({ ...prev, name: validateSkillName(raw) }));
                        }}
                        maxLength={64}
                        className={`w-full px-3 py-2 rounded-md bg-bg-sidebar border outline-none text-[13px] text-text-base placeholder-text-muted/40 font-mono transition-colors ${
                          fieldErrors.name ? "border-red-500/60 focus:border-red-500" : "border-border-strong/40 hover:border-border-strong focus:border-brand"
                        }`}
                        spellCheck={false}
                      />
                      {fieldErrors.name && (
                        <p className="mt-1.5 text-[11px] text-red-400">{fieldErrors.name}</p>
                      )}
                    </div>

                    {/* description */}
                    <div>
                      <div className="flex items-baseline justify-between mb-1.5">
                        <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                          Description <span className="text-red-400 ml-0.5">*</span>
                        </label>
                        <span className={`text-[11px] tabular-nums ${editDescription.length > 900 ? (editDescription.length > 1024 ? "text-red-400" : "text-warning") : "text-text-muted"}`}>
                          {editDescription.length}/1024
                        </span>
                      </div>
                      <textarea
                        placeholder="A concise description of what this skill does and when to use it."
                        value={editDescription}
                        onChange={(e) => {
                          setEditDescription(e.target.value);
                          setFieldErrors(prev => ({ ...prev, description: validateSkillDescription(e.target.value) }));
                        }}
                        rows={3}
                        maxLength={1024}
                        className={`w-full px-3 py-2 rounded-md bg-bg-sidebar border outline-none text-[13px] text-text-base placeholder-text-muted/40 resize-none transition-colors leading-relaxed ${
                          fieldErrors.description ? "border-red-500/60 focus:border-red-500" : "border-border-strong/40 hover:border-border-strong focus:border-brand"
                        }`}
                        spellCheck={false}
                      />
                      {fieldErrors.description && (
                        <p className="mt-1 text-[11px] text-red-400">{fieldErrors.description}</p>
                      )}
                    </div>
                  </div>

                  {/* Body textarea */}
                  <div className="flex-1 min-h-0 flex flex-col">
                    <div className="px-6 pt-3 pb-2 shrink-0">
                      <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                        Body
                      </label>
                    </div>
                    <LineNumberedTextarea
                      value={editBody}
                      onChange={setEditBody}
                      className="flex-1"
                    />
                  </div>
                </>
              ) : skillContent ? (
                /* Rich preview — skill content + right "Used By" sidebar */
                <div className="flex h-full min-h-0">
                  {/* Main scrollable content */}
                  <div className="flex-1 overflow-y-auto custom-scrollbar min-w-0">
                    <SkillPreview
                      content={skillContent}
                      source={selectedEntry?.source}
                      sources={selectedEntry?.sources}
                      resources={skillResources}
                      license={selectedEntry?.license}
                      onUpdated={async (name) => {
                        // Refresh library list + the displayed skill so the
                        // new SHA, version, and content are reflected.
                        await loadSkills();
                        await loadSkillContent(name);
                      }}
                    />
                  </div>

                  {/* ── Right "Used By" sidebar ─────────────────────────── */}
                  {skillUsedBy && (skillUsedBy.projects.length > 0 || skillUsedBy.templates.length > 0) && (
                    <div className="w-52 flex-shrink-0 border-l border-border-strong/40 bg-bg-input/40 flex flex-col overflow-y-auto custom-scrollbar">
                      <div className="px-3 py-2.5 border-b border-border-strong/40 shrink-0">
                        <p className="text-[10px] font-semibold text-text-muted tracking-wider uppercase">Used By</p>
                      </div>

                      <div className="flex-1 py-1">
                        {/* Projects section */}
                        {skillUsedBy.projects.length > 0 && (
                          <div>
                            <p className="px-3 pt-2 pb-1 text-[10px] font-semibold text-text-muted/70 tracking-wider uppercase">
                              Projects
                            </p>
                            {skillUsedBy.projects.map(name => (
                              <button
                                key={`project-${name}`}
                                onClick={() => onNavigateToProject?.(name)}
                                className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-bg-sidebar/60 transition-colors group"
                                title={`Open project: ${name}`}
                              >
                                <FolderOpen size={12} className="shrink-0 text-brand" />
                                <span className="flex-1 text-[12px] text-text-base truncate group-hover:text-brand transition-colors">{name}</span>
                              </button>
                            ))}
                          </div>
                        )}

                        {/* Templates section */}
                        {skillUsedBy.templates.length > 0 && (
                          <div className={skillUsedBy.projects.length > 0 ? "mt-1" : ""}>
                            <p className="px-3 pt-2 pb-1 text-[10px] font-semibold text-text-muted/70 tracking-wider uppercase">
                              Templates
                            </p>
                            {skillUsedBy.templates.map(name => (
                              <button
                                key={`template-${name}`}
                                onClick={() => onNavigateToTemplate?.(name)}
                                className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-bg-sidebar/60 transition-colors group"
                                title={`Open template: ${name}`}
                              >
                                <LayoutTemplate size={12} className="shrink-0 text-accent" />
                                <span className="flex-1 text-[12px] text-text-base truncate group-hover:text-accent transition-colors">{name}</span>
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              ) : (
                <div className="h-full flex items-center justify-center">
                  <span className="text-[13px] text-text-muted italic">This skill is empty. Click Edit to add instructions.</span>
                </div>
              )}
            </div>
          </div>
        ) : null}
      </AssetDrawer>

      <SkillImportDialog
        isOpen={showImportDialog}
        onClose={() => setShowImportDialog(false)}
        onImport={async (skillName) => {
          // Refresh the skills list and selected-skill content in the
          // background. The dialog stays open so the user can read the
          // import summary; closing is the user's call via "Done".
          await loadSkills();
          await loadSkillContent(skillName);
          setRecentRefresh(prev => prev + 1);
        }}
      />
    </div>
  );
}
