import React, { useState, useEffect, useRef, useCallback } from "react";
import { MarkdownPreview } from "./MarkdownPreview";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  trackSkillCreated,
  trackSkillUpdated,
  trackSkillDeleted,
  trackSkillSynced,
  trackAllSkillsSynced,
} from "./analytics";
import {
  Plus,
  X,
  Edit2,
  Code,
  FileText,
  Check,
  RefreshCw,
  Globe,
  HardDrive,
  Github,
  Search,
} from "lucide-react";
import { ICONS } from "./icons";

interface SkillSource {
  source: string; // "owner/repo"
  id: string;     // "owner/repo/skill-name"
}

interface SkillEntry {
  name: string;
  in_agents: boolean;
  in_claude: boolean;
  source?: SkillSource;
  has_resources: boolean;
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

const SIDEBAR_MIN = 240;
const SIDEBAR_MAX = 480;
const SIDEBAR_DEFAULT = 340;

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

// ── Skill preview — frontmatter header + companion resources + markdown body ──

interface SkillPreviewProps {
  content: string;
  source?: SkillSource;
  resources?: SkillResources | null;
}

function SkillPreview({ content, source, resources }: SkillPreviewProps) {
  const { meta, body } = parseFrontmatter(content);
  const displayName = meta.name || "";
  const description = meta.description || "";

  const hasResources =
    resources && (resources.dirs.length > 0 || resources.root_files.length > 0);

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

  return (
    <div>
      {/* ── Metadata header ───────────────────────────────────────────── */}
      <div className="px-8 pt-6 pb-0">
        {displayName && (
          <h1 className="text-[20px] font-semibold text-text-base mb-2 leading-tight">{displayName}</h1>
        )}
        {description && (
          <p className="text-[13px] text-text-muted leading-relaxed mb-4">{description}</p>
        )}

        {/* Source link (remote skills only) */}
        {source && (
          <div className="mb-4">
            <a
              href={`https://github.com/${source.source}`}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 text-[12px] text-text-base hover:text-text-base transition-colors"
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor" className="opacity-70">
                <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z"/>
              </svg>
              {source.source}
            </a>
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
                      <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" className="shrink-0 text-brand-light">
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

        {(displayName || description || source || hasResources) && (
          <div className="border-b border-border-strong/40 mb-0" />
        )}
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
  const safeDesc = description.includes(":") ? `"${description.replace(/"/g, '\\"')}"` : description;
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

// ── Main Component ────────────────────────────────────────────────────────────

export default function Skills() {
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [newSkillName, setNewSkillName] = useState("");
  const [newSkillDescription, setNewSkillDescription] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [filter, setFilter] = useState<"all" | "remote" | "local">("all");
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [syncingSkill, setSyncingSkill] = useState<string | null>(null);
  const [syncingAll, setSyncingAll] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT);
  const isDragging = useRef(false);

  // Companion resources for the selected skill
  const [skillResources, setSkillResources] = useState<SkillResources | null>(null);

  // Frontmatter field state for the edit panel (existing skills)
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [fieldErrors, setFieldErrors] = useState<FieldError>({ name: null, description: null });
  // Body content without frontmatter, for the split editor
  const [editBody, setEditBody] = useState("");

  useEffect(() => { loadSkills(); }, []);

  // ── Resize ────────────────────────────────────────────────────────────────

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isDragging.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, []);

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!isDragging.current) return;
      setSidebarWidth(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, e.clientX - 180)));
    };
    const onMouseUp = () => {
      if (isDragging.current) {
        isDragging.current = false;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => { window.removeEventListener("mousemove", onMouseMove); window.removeEventListener("mouseup", onMouseUp); };
  }, []);

  // ── Data ──────────────────────────────────────────────────────────────────

  const loadSkills = async () => {
    try {
      const result: SkillEntry[] = await invoke("get_skills");
      setSkills(result.sort((a, b) => a.name.localeCompare(b.name)));
      setError(null);
    } catch (err: any) {
      setError(`Failed to load skills: ${err}`);
    }
  };

  const loadSkillContent = async (name: string) => {
    try {
      const [content, resources] = await Promise.all([
        invoke<string>("read_skill", { name }),
        invoke<SkillResources>("get_skill_resources", { name }),
      ]);
      setSelectedSkill(name);
      setSkillContent(content);
      setSkillResources(resources);
      // Parse frontmatter into edit fields
      const { meta, body } = parseFrontmatter(content);
      setEditName(meta.name ?? name);
      setEditDescription(meta.description ?? "");
      setEditBody(body);
      setFieldErrors({ name: null, description: null });
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
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
        await invoke("save_skill", { name: newSkillName, content });
        trackSkillCreated(newSkillName, "local");
        setIsCreating(false);
        setIsEditing(false);
        await loadSkills();
        await loadSkillContent(newSkillName);
        setError(null);
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
        await invoke("save_skill", { name: selectedSkill!, content: finalContent });
        trackSkillUpdated(selectedSkill!);
        setSkillContent(finalContent);
        setIsEditing(false);
        setError(null);
      } catch (err: any) {
        setError(`Failed to save skill: ${err}`);
      }
    }
  };

  const handleDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete skill "${name}"?`, { title: "Delete Skill", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_skill", { name });
      trackSkillDeleted(name);
      if (selectedSkill === name) { setSelectedSkill(null); setSkillContent(""); setIsEditing(false); }
      await loadSkills();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete skill: ${err}`);
    }
  };

  const handleSyncSkill = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setSyncingSkill(name);
    try {
      await invoke("sync_skill", { name });
      trackSkillSynced(name);
      await loadSkills();
      setError(null);
    } catch (err: any) {
      setError(`Failed to sync skill: ${err}`);
    } finally { setSyncingSkill(null); }
  };

  const handleSyncAll = async () => {
    setSyncingAll(true);
    try {
      await invoke("sync_all_skills");
      await loadSkills();
      trackAllSkillsSynced(skills.length);
      setError(null);
    } catch (err: any) {
      setError(`Failed to sync skills: ${err}`);
    } finally { setSyncingAll(false); }
  };

  const startCreateNew = () => {
    setSelectedSkill(null);
    setNewSkillName("");
    setNewSkillDescription("");
    setEditBody(DEFAULT_SKILL_BODY);
    setSkillContent("");
    setSkillResources(null);
    setFieldErrors({ name: null, description: null });
    setIsCreating(true);
    setIsEditing(true);
  };

  // ── Derived ───────────────────────────────────────────────────────────────

  const isSynced = (skill: SkillEntry) => skill.in_agents && skill.in_claude;
  const unsyncedCount = skills.filter(s => !isSynced(s)).length;
  const remoteCount = skills.filter(s => !!s.source).length;
  const localCount = skills.length - remoteCount;

  const searchLower = search.trim().toLowerCase();
  const filteredSkills = skills.filter(s => {
    if (filter === "remote" && !s.source) return false;
    if (filter === "local" && !!s.source) return false;
    if (searchLower && !s.name.toLowerCase().includes(searchLower)) return false;
    return true;
  });

  const selectedEntry = skills.find(s => s.name === selectedSkill);

  return (
    <div className="flex h-full w-full bg-bg-base">

      {/* ── Left Sidebar ─────────────────────────────────────────────────── */}
      <div
        className="flex-shrink-0 flex flex-col border-r border-border-strong/40 bg-bg-input/50 relative"
        style={{ width: sidebarWidth }}
      >
        {/* Header */}
        <div className="px-4 pt-4 pb-3 border-b border-border-strong/40">
          <div className="flex items-center justify-between mb-3">
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
              Skills
            </span>
            <div className="flex items-center gap-1">
              {unsyncedCount > 0 && (
                <button
                  onClick={handleSyncAll}
                  disabled={syncingAll}
                  className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded disabled:opacity-50"
                  title={`Sync all (${unsyncedCount} unsynced)`}
                >
                  <RefreshCw size={13} className={syncingAll ? "animate-spin" : ""} />
                </button>
              )}
              <button
                onClick={startCreateNew}
                className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
                title="New Skill"
              >
                <Plus size={14} />
              </button>
            </div>
          </div>

          {/* Filter tabs */}
          <div className="flex gap-1 mb-2">
            {(["all", "remote", "local"] as const).map(f => (
              <button
                key={f}
                onClick={() => setFilter(f)}
                className={`flex-1 py-1 rounded text-[11px] font-medium transition-colors ${
                  filter === f
                    ? "bg-bg-sidebar text-text-base"
                    : "text-text-muted hover:text-text-base"
                }`}
              >
                {f === "all" ? `All ${skills.length}` : f === "remote" ? `Remote ${remoteCount}` : `Local ${localCount}`}
              </button>
            ))}
          </div>

          {/* Search */}
          <div className="relative">
            <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
            <input
              type="text"
              placeholder="Search skills…"
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="w-full pl-7 pr-7 py-1.5 rounded bg-bg-sidebar border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[12px] text-text-base placeholder-text-muted/60 transition-colors"
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
        </div>

        {/* List */}
        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {filteredSkills.length === 0 && !isCreating ? (
            <div className="px-4 py-6 text-center">
              <p className="text-[13px] text-text-muted">
                {searchLower ? `No skills match "${search}".` : "No skills found."}
              </p>
            </div>
          ) : (
            <ul className="space-y-px px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-md text-[13px] bg-bg-sidebar text-text-base">
                  <div className={ICONS.skill.iconBox}>
                    <Code size={15} className={ICONS.skill.iconColor} />
                  </div>
                  <span className={newSkillName ? "text-text-base font-medium" : "italic text-text-muted"}>
                    {newSkillName || "New Skill…"}
                  </span>
                </li>
              )}
              {filteredSkills.map(skill => {
                const isSelected = selectedSkill === skill.name && !isCreating;
                const synced = isSynced(skill);
                const isRemote = !!skill.source;

                return (
                  <li key={skill.name} className="group">
                    <button
                      onClick={() => loadSkillContent(skill.name)}
                      className={`w-full text-left flex items-center gap-3 px-3 py-2.5 rounded-md transition-colors ${
                        isSelected ? "bg-bg-sidebar" : "hover:bg-bg-sidebar/50"
                      }`}
                    >
                      <div className={ICONS.skill.iconBox}>
                        <Code size={15} className={ICONS.skill.iconColor} />
                      </div>
                      <div className="flex-1 min-w-0">
                      {/* Top row: name + action buttons */}
                      <div className="flex items-center gap-2">
                        <span className={`flex-1 text-[13px] font-medium truncate min-w-0 ${isSelected ? "text-text-base" : "text-text-base group-hover:text-text-base"}`}>
                          {skill.name}
                        </span>
                        {/* Hover actions */}
                        <span className="shrink-0 hidden group-hover:flex items-center gap-0.5">
                          {!synced && (
                            <span
                              role="button"
                              onClick={(e) => handleSyncSkill(skill.name, e)}
                              className="p-0.5 text-text-muted hover:text-brand rounded transition-colors"
                              title="Sync to both locations"
                            >
                              <RefreshCw size={11} className={syncingSkill === skill.name ? "animate-spin" : ""} />
                            </span>
                          )}
                          <span
                            role="button"
                            onClick={(e) => handleDelete(skill.name, e)}
                            className="p-0.5 text-text-muted hover:text-danger rounded transition-colors"
                            title="Delete"
                          >
                            <X size={11} />
                          </span>
                        </span>
                      </div>

                      {/* Bottom row: origin + location badges */}
                      <div className="flex items-center gap-1.5 mt-1">
                        {isRemote ? (
                          <span className="flex items-center gap-1 text-[10px] text-success">
                            <Globe size={9} />
                            <span className="truncate max-w-[120px]">{skill.source!.source}</span>
                          </span>
                        ) : (
                          <span className="flex items-center gap-1 text-[10px] text-text-muted">
                            <HardDrive size={9} />
                            <span>local</span>
                          </span>
                        )}
                        <span className="text-[10px] text-text-muted">·</span>
                        {skill.in_agents && (
                          <span className="text-[9px] font-semibold px-1 py-0.5 rounded bg-brand/20 text-brand-light leading-none" title="~/.agents/skills/">A</span>
                        )}
                        {skill.in_claude && (
                          <span className="text-[9px] font-semibold px-1 py-0.5 rounded bg-[#D2875E]/20 text-[#E6A87B] leading-none" title="~/.claude/skills/">C</span>
                        )}
                        {!synced && (
                          <span className="text-[9px] font-semibold px-1 py-0.5 rounded bg-warning/15 text-warning leading-none" title="Not synced to both locations">!</span>
                        )}
                        {skill.has_resources && (
                          <span title="Has additional resources">
                            <svg width="9" height="9" viewBox="0 0 16 16" fill="currentColor" className="text-text-muted">
                              <path d="M1.75 1A1.75 1.75 0 0 0 0 2.75v10.5C0 14.216.784 15 1.75 15h12.5A1.75 1.75 0 0 0 16 13.25v-8.5A1.75 1.75 0 0 0 14.25 3H7.5a.25.25 0 0 1-.2-.1l-.9-1.2C6.07 1.26 5.55 1 5 1H1.75z"/>
                            </svg>
                          </span>
                        )}
                      </div>
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        {/* Resize handle */}
        <div
          onMouseDown={onMouseDown}
          className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-brand/40 active:bg-brand/60 transition-colors z-10"
        />
      </div>

      {/* ── Right Pane ───────────────────────────────────────────────────── */}
      <div className="flex-1 flex flex-col min-w-0">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between shrink-0">
            {error}
            <button onClick={() => setError(null)}><X size={14} /></button>
          </div>
        )}

        {isCreating ? (
          /* ── New Skill Form ─────────────────────────────────────────────── */
          <div className="flex-1 flex flex-col h-full min-h-0">

            {/* Header */}
            <div className="h-11 px-5 border-b border-border-strong/40 flex items-center justify-between shrink-0">
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
                    Lowercase letters, digits, and hyphens only. Becomes the directory name under <code className="font-mono">~/.agents/skills/</code>.
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
              <textarea
                value={editBody}
                onChange={(e) => setEditBody(e.target.value)}
                className="flex-1 px-6 pb-6 resize-none outline-none font-mono text-[13px] bg-bg-base text-text-base leading-relaxed custom-scrollbar"
                spellCheck={false}
              />
            </div>
          </div>

        ) : selectedSkill ? (
          /* ── Existing Skill View/Edit ────────────────────────────────────── */
          <div className="flex-1 flex flex-col h-full min-h-0">

            {/* Header */}
            <div className="h-11 px-5 border-b border-border-strong/40 flex items-center justify-between shrink-0">
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
                        className="flex items-center gap-1 text-[11px] text-text-muted hover:text-text-base transition-colors truncate"
                        onClick={e => e.stopPropagation()}
                      >
                        <Github size={11} />
                        {selectedEntry.source.source}
                      </a>
                    </>
                  )}
                </>
              </div>

              <div className="flex items-center gap-2 shrink-0">
                {!isEditing && (
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
                    <textarea
                      value={editBody}
                      onChange={(e) => setEditBody(e.target.value)}
                      className="flex-1 px-6 pb-6 resize-none outline-none font-mono text-[13px] bg-bg-base text-text-base leading-relaxed custom-scrollbar"
                      spellCheck={false}
                    />
                  </div>
                </>
              ) : skillContent ? (
                /* Rich preview for all skills */
                <div className="h-full overflow-y-auto custom-scrollbar">
                  <SkillPreview
                    content={skillContent}
                    source={selectedEntry?.source}
                    resources={skillResources}
                  />
                </div>
              ) : (
                <div className="h-full flex items-center justify-center">
                  <span className="text-[13px] text-text-muted italic">This skill is empty. Click Edit to add instructions.</span>
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-icon-skill/12 border border-icon-skill/20 flex items-center justify-center">
              <Code size={22} className={ICONS.skill.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">
              {skills.length === 0 ? "No skills yet" : "No skill selected"}
            </h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              {skills.length === 0
                ? "Skills are reusable instruction sets that agents load on demand. Create your first skill to get started."
                : "Select a skill from the list to view its contents, or create a new one."}
            </p>
            <button
              onClick={startCreateNew}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
            >
              <Plus size={14} />
              New Skill
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
