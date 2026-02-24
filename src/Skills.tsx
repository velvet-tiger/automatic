import React, { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
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
}

const SIDEBAR_MIN = 240;
const SIDEBAR_MAX = 480;
const SIDEBAR_DEFAULT = 300;

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

// ── Simple markdown renderer (headings + paragraphs only, for view mode) ─────

function SkillPreview({ content }: { content: string }) {
  const { meta, body } = parseFrontmatter(content);
  const displayName = meta.name || "";
  const description = meta.description || "";

  // Inline formatter
  function inlineFormat(text: string): React.ReactElement {
    const parts: (string | React.ReactElement)[] = [];
    const regex = /(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|_[^_]+_|\*[^*]+\*)/g;
    let last = 0;
    let key = 0;
    let m: RegExpExecArray | null;
    while ((m = regex.exec(text)) !== null) {
      if (m.index > last) parts.push(text.slice(last, m.index));
      const t = m[0]!;
      if (t.startsWith("`"))
        parts.push(<code key={key++} className="px-1 py-0.5 rounded bg-[#1A1A1E] text-[#5E6AD2] font-mono text-[11px]">{t.slice(1, -1)}</code>);
      else if (t.startsWith("**") || t.startsWith("__"))
        parts.push(<strong key={key++} className="font-semibold text-[#E0E1E6]">{t.slice(2, -2)}</strong>);
      else
        parts.push(<em key={key++} className="italic text-[#C0C1C6]">{t.slice(1, -1)}</em>);
      last = m.index + t.length;
    }
    if (last < text.length) parts.push(text.slice(last));
    return <span>{parts}</span>;
  }

  // Body renderer
  const lines = body.split("\n");
  const elements: React.ReactElement[] = [];
  let i = 0;
  let k = 0;

  while (i < lines.length) {
    const line = lines[i]!;

    if (line.startsWith("```")) {
      const lang = line.slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i]!.startsWith("```")) { codeLines.push(lines[i]!); i++; }
      elements.push(
        <pre key={k++} className="mt-2 mb-4 bg-[#1A1A1E] border border-[#33353A] rounded-md px-4 py-3 font-mono text-[11px] text-[#C0C1C6] leading-relaxed overflow-x-auto whitespace-pre">
          {lang && <span className="block text-[10px] text-[#8A8C93] mb-2 uppercase tracking-wider">{lang}</span>}
          {codeLines.join("\n")}
        </pre>
      );
      i++; continue;
    }
    if (line.startsWith("# ")) { elements.push(<h1 key={k++} className="text-[18px] font-semibold text-[#E0E1E6] mt-5 mb-2">{inlineFormat(line.slice(2))}</h1>); i++; continue; }
    if (line.startsWith("## ")) { elements.push(<h2 key={k++} className="text-[14px] font-semibold text-[#E0E1E6] mt-5 mb-2 pb-1.5 border-b border-[#33353A]">{inlineFormat(line.slice(3))}</h2>); i++; continue; }
    if (line.startsWith("### ")) { elements.push(<h3 key={k++} className="text-[13px] font-semibold text-[#E0E1E6] mt-4 mb-1.5">{inlineFormat(line.slice(4))}</h3>); i++; continue; }

    if (line.includes("|") && i + 1 < lines.length && /^\|?[\s|:-]+\|/.test(lines[i + 1]!)) {
      const parseRow = (row: string) => row.replace(/^\|/, "").replace(/\|$/, "").split("|").map(c => c.trim());
      const headers = parseRow(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && lines[i]!.includes("|")) { rows.push(parseRow(lines[i]!)); i++; }
      elements.push(
        <div key={k++} className="my-4 overflow-x-auto">
          <table className="w-full text-[12px] border-collapse">
            <thead><tr>{headers.map((h, hi) => <th key={hi} className="text-left px-3 py-2 text-[#E0E1E6] font-semibold border-b border-[#33353A] bg-[#1A1A1E] whitespace-nowrap">{inlineFormat(h)}</th>)}</tr></thead>
            <tbody>{rows.map((cells, ri) => <tr key={ri} className={ri % 2 === 0 ? "bg-[#222327]" : "bg-[#1E1F23]"}>{cells.map((cell, ci) => <td key={ci} className="px-3 py-2 text-[#C0C1C6] border-b border-[#33353A]/40 align-top">{inlineFormat(cell)}</td>)}</tr>)}</tbody>
          </table>
        </div>
      );
      continue;
    }

    if (/^[-*] /.test(line)) {
      const items: React.ReactElement[] = [];
      while (i < lines.length && /^[-*] /.test(lines[i]!)) {
        items.push(<li key={k++} className="flex gap-2 text-[13px] text-[#C0C1C6] leading-relaxed"><span className="mt-1.5 w-1 h-1 rounded-full bg-[#8A8C93] flex-shrink-0" /><span>{inlineFormat(lines[i]!.replace(/^[-*] /, ""))}</span></li>);
        i++;
      }
      elements.push(<ul key={k++} className="my-2 space-y-1.5 pl-1">{items}</ul>);
      continue;
    }
    if (/^\d+\. /.test(line)) {
      const items: React.ReactElement[] = [];
      let n = 1;
      while (i < lines.length && /^\d+\. /.test(lines[i]!)) {
        items.push(<li key={k++} className="flex gap-2.5 text-[13px] text-[#C0C1C6] leading-relaxed"><span className="flex-shrink-0 text-[11px] text-[#8A8C93] font-mono w-4 text-right mt-0.5">{n}.</span><span>{inlineFormat(lines[i]!.replace(/^\d+\. /, ""))}</span></li>);
        i++; n++;
      }
      elements.push(<ol key={k++} className="my-2 space-y-1.5">{items}</ol>);
      continue;
    }
    if (/^---+$/.test(line.trim())) { elements.push(<hr key={k++} className="my-4 border-[#33353A]" />); i++; continue; }
    if (line.trim() === "") { i++; continue; }
    elements.push(<p key={k++} className="text-[13px] text-[#C0C1C6] leading-relaxed my-2">{inlineFormat(line)}</p>);
    i++;
  }

  return (
    <div className="px-8 py-6">
      {displayName && (
        <h1 className="text-[20px] font-semibold text-[#E0E1E6] mb-2 leading-tight">{displayName}</h1>
      )}
      {description && (
        <p className="text-[13px] text-[#8A8C93] leading-relaxed mb-6 pb-5 border-b border-[#33353A]">{description}</p>
      )}
      <div>{elements}</div>
    </div>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

export default function Skills() {
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [newSkillName, setNewSkillName] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [filter, setFilter] = useState<"all" | "remote" | "local">("all");
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [syncingSkill, setSyncingSkill] = useState<string | null>(null);
  const [syncingAll, setSyncingAll] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT);
  const isDragging = useRef(false);

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
      const content: string = await invoke("read_skill", { name });
      setSelectedSkill(name);
      setSkillContent(content);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
    } catch (err: any) {
      setError(`Failed to read skill ${name}: ${err}`);
    }
  };

  const handleSave = async () => {
    if (!selectedSkill) return;
    try {
      await invoke("save_skill", { name: selectedSkill, content: skillContent });
      setIsEditing(false);
      if (isCreating) { setIsCreating(false); await loadSkills(); }
      setError(null);
    } catch (err: any) {
      setError(`Failed to save skill: ${err}`);
    }
  };

  const handleDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invoke("delete_skill", { name });
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
      setError(null);
    } catch (err: any) {
      setError(`Failed to sync skills: ${err}`);
    } finally { setSyncingAll(false); }
  };

  const startCreateNew = () => {
    setSelectedSkill(null);
    setSkillContent("");
    setIsCreating(true);
    setIsEditing(true);
    setNewSkillName("");
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
    <div className="flex h-full w-full bg-[#222327]">

      {/* ── Left Sidebar ─────────────────────────────────────────────────── */}
      <div
        className="flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50 relative"
        style={{ width: sidebarWidth }}
      >
        {/* Header */}
        <div className="px-4 pt-4 pb-3 border-b border-[#33353A]">
          <div className="flex items-center justify-between mb-3">
            <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
              Skills
            </span>
            <div className="flex items-center gap-1">
              {unsyncedCount > 0 && (
                <button
                  onClick={handleSyncAll}
                  disabled={syncingAll}
                  className="text-[#8A8C93] hover:text-[#E0E1E6] transition-colors p-1 hover:bg-[#2D2E36] rounded disabled:opacity-50"
                  title={`Sync all (${unsyncedCount} unsynced)`}
                >
                  <RefreshCw size={13} className={syncingAll ? "animate-spin" : ""} />
                </button>
              )}
              <button
                onClick={startCreateNew}
                className="text-[#8A8C93] hover:text-[#E0E1E6] transition-colors p-1 hover:bg-[#2D2E36] rounded"
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
                    ? "bg-[#2D2E36] text-[#E0E1E6]"
                    : "text-[#8A8C93] hover:text-[#E0E1E6]"
                }`}
              >
                {f === "all" ? `All ${skills.length}` : f === "remote" ? `Remote ${remoteCount}` : `Local ${localCount}`}
              </button>
            ))}
          </div>

          {/* Search */}
          <div className="relative">
            <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[#8A8C93] pointer-events-none" />
            <input
              type="text"
              placeholder="Search skills…"
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="w-full pl-7 pr-7 py-1.5 rounded bg-[#2D2E36] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] outline-none text-[12px] text-[#E0E1E6] placeholder-[#8A8C93]/60 transition-colors"
            />
            {search && (
              <button
                onClick={() => setSearch("")}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
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
              <p className="text-[13px] text-[#8A8C93]">
                {searchLower ? `No skills match "${search}".` : "No skills found."}
              </p>
            </div>
          ) : (
            <ul className="space-y-px px-2">
              {isCreating && (
                <li className="flex items-center gap-2.5 px-3 py-2 rounded-md text-[13px] bg-[#2D2E36] text-[#E0E1E6]">
                  <Code size={13} className={`${ICONS.skill.iconColor} shrink-0`} />
                  <span className="italic text-[#8A8C93]">New Skill…</span>
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
                      className={`w-full text-left px-3 py-2.5 rounded-md transition-colors ${
                        isSelected ? "bg-[#2D2E36]" : "hover:bg-[#2D2E36]/50"
                      }`}
                    >
                      {/* Top row: name + action buttons */}
                      <div className="flex items-center gap-2">
                        <span className={`flex-1 text-[13px] font-medium truncate min-w-0 ${isSelected ? "text-[#E0E1E6]" : "text-[#C0C1C6] group-hover:text-[#E0E1E6]"}`}>
                          {skill.name}
                        </span>
                        {/* Hover actions */}
                        <span className="shrink-0 hidden group-hover:flex items-center gap-0.5">
                          {!synced && (
                            <span
                              role="button"
                              onClick={(e) => handleSyncSkill(skill.name, e)}
                              className="p-0.5 text-[#8A8C93] hover:text-[#5E6AD2] rounded transition-colors"
                              title="Sync to both locations"
                            >
                              <RefreshCw size={11} className={syncingSkill === skill.name ? "animate-spin" : ""} />
                            </span>
                          )}
                          <span
                            role="button"
                            onClick={(e) => handleDelete(skill.name, e)}
                            className="p-0.5 text-[#8A8C93] hover:text-[#FF6B6B] rounded transition-colors"
                            title="Delete"
                          >
                            <X size={11} />
                          </span>
                        </span>
                      </div>

                      {/* Bottom row: origin + location badges */}
                      <div className="flex items-center gap-1.5 mt-1">
                        {isRemote ? (
                          <span className="flex items-center gap-1 text-[10px] text-[#4ADE80]">
                            <Globe size={9} />
                            <span className="truncate max-w-[120px]">{skill.source!.source}</span>
                          </span>
                        ) : (
                          <span className="flex items-center gap-1 text-[10px] text-[#8A8C93]">
                            <HardDrive size={9} />
                            <span>local</span>
                          </span>
                        )}
                        <span className="text-[10px] text-[#8A8C93]/30">·</span>
                        {skill.in_agents && (
                          <span className="text-[9px] font-semibold px-1 py-0.5 rounded bg-[#5E6AD2]/20 text-[#8B93E6] leading-none" title="~/.agents/skills/">A</span>
                        )}
                        {skill.in_claude && (
                          <span className="text-[9px] font-semibold px-1 py-0.5 rounded bg-[#D2875E]/20 text-[#E6A87B] leading-none" title="~/.claude/skills/">C</span>
                        )}
                        {!synced && (
                          <span className="text-[9px] font-semibold px-1 py-0.5 rounded bg-[#F59E0B]/15 text-[#F59E0B] leading-none" title="Not synced to both locations">!</span>
                        )}
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
          className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-[#5E6AD2]/40 active:bg-[#5E6AD2]/60 transition-colors z-10"
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

        {(selectedSkill || isCreating) ? (
          <div className="flex-1 flex flex-col h-full min-h-0">

            {/* Header */}
            <div className="h-11 px-5 border-b border-[#33353A] flex items-center justify-between shrink-0">
              <div className="flex items-center gap-2.5 min-w-0">
                <FileText size={13} className={`${ICONS.skill.iconColor} shrink-0`} />
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="skill-name (lowercase, hyphens)"
                    value={newSkillName}
                    onChange={(e) => { setNewSkillName(e.target.value); setSelectedSkill(e.target.value); }}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-[#E0E1E6] placeholder-[#8A8C93]/50 w-64"
                  />
                ) : (
                  <>
                    <h3 className="text-[14px] font-medium text-[#E0E1E6] truncate">{selectedSkill}</h3>
                    {selectedEntry?.source && (
                      <>
                        <span className="text-[#33353A]">/</span>
                        <a
                          href={`https://github.com/${selectedEntry.source.source}`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="flex items-center gap-1 text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors truncate"
                          onClick={e => e.stopPropagation()}
                        >
                          <Github size={11} />
                          {selectedEntry.source.source}
                        </a>
                      </>
                    )}
                  </>
                )}
              </div>

              <div className="flex items-center gap-2 shrink-0">
                {!isCreating && !isEditing && (
                  <button
                    onClick={() => setIsEditing(true)}
                    className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-[#2D2E36] text-[#8A8C93] hover:text-[#E0E1E6] rounded text-[12px] font-medium transition-colors"
                  >
                    <Edit2 size={12} /> Edit
                  </button>
                )}
                {isEditing && (
                  <>
                    {!isCreating && (
                      <button
                        onClick={() => { setIsEditing(false); loadSkillContent(selectedSkill!); }}
                        className="px-3 py-1.5 hover:bg-[#2D2E36] text-[#8A8C93] hover:text-[#E0E1E6] rounded text-[12px] font-medium transition-colors"
                      >
                        Cancel
                      </button>
                    )}
                    <button
                      onClick={handleSave}
                      disabled={isCreating && !newSkillName.trim()}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      <Check size={12} /> Save
                    </button>
                  </>
                )}
              </div>
            </div>

            {/* Body */}
            <div className="flex-1 min-h-0 overflow-hidden">
              {isEditing ? (
                <textarea
                  value={skillContent}
                  onChange={(e) => setSkillContent(e.target.value)}
                  className="w-full h-full p-6 resize-none outline-none font-mono text-[13px] bg-[#222327] text-[#E0E1E6] leading-relaxed custom-scrollbar placeholder-[#8A8C93]/30"
                  placeholder="Write your skill instructions here in Markdown…"
                  spellCheck={false}
                />
              ) : skillContent ? (
                /* Rich preview for all skills */
                <div className="h-full overflow-y-auto custom-scrollbar">
                  <SkillPreview content={skillContent} />
                </div>
              ) : (
                <div className="h-full flex items-center justify-center">
                  <span className="text-[13px] text-[#8A8C93] italic">This skill is empty. Click Edit to add instructions.</span>
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-[#4ADE80]/10 border border-[#4ADE80]/20 flex items-center justify-center">
              <Code size={22} className={ICONS.skill.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-[#E0E1E6] mb-2">No skill selected</h2>
            <p className="text-[13px] text-[#8A8C93] leading-relaxed max-w-xs">
              Select a skill to view its contents, or create a new one.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
