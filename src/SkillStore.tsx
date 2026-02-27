import React, { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { trackSkillInstalled, trackSkillUpdated } from "./analytics";
import {
  Search,
  Download,
  ExternalLink,
  Loader2,
  CheckCircle2,
  Github,
  Package,
  ArrowRight,
  ArrowLeft,
} from "lucide-react";
import featuredSkillsData from "./featured-skills.json";

interface RemoteSkillResult {
  id: string;
  name: string;
  installs: number;
  source: string;
}

function formatInstalls(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1).replace(/\.0$/, "")}k`;
  return `${count}`;
}

// ── Frontmatter parser ──────────────────────────────────────────────────────

interface Frontmatter {
  name?: string;
  description?: string;
  [key: string]: string | undefined;
}

function parseFrontmatter(raw: string): { meta: Frontmatter; body: string } {
  const match = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!match) return { meta: {}, body: raw };

  const yamlBlock = match[1]!;
  const body = match[2]!.trimStart();
  const meta: Frontmatter = {};

  const lines = yamlBlock.split("\n");
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
      // Collect indented continuation lines
      while (i < lines.length && (lines[i]!.startsWith(" ") || lines[i]!.startsWith("\t"))) {
        blockLines.push(lines[i]!.trim());
        i++;
      }
      // `>` folds newlines into spaces; `|` preserves them — for description we just want the text
      meta[key] = blockLines.join(rest.startsWith("|") ? "\n" : " ");
    } else {
      meta[key] = rest.replace(/^["']|["']$/g, "");
      i++;
    }
  }

  return { meta, body };
}

// ── Markdown renderer ───────────────────────────────────────────────────────
// Lightweight line-by-line renderer — no external dependency needed.

function renderMarkdown(md: string): React.ReactElement {
  const lines = md.split("\n");
  const elements: React.ReactElement[] = [];
  let i = 0;
  let key = 0;

  const nextKey = () => key++;

  // Inline formatting: **bold**, `code`, _italic_
  function inlineFormat(text: string): React.ReactElement {
    const parts: (string | React.ReactElement)[] = [];
    const regex = /(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|_[^_]+_|\*[^*]+\*)/g;
    let last = 0;
    let m: RegExpExecArray | null;

    while ((m = regex.exec(text)) !== null) {
      if (m.index > last) parts.push(text.slice(last, m.index));
      const token = m[0];
      if (token.startsWith("`")) {
        parts.push(
          <code
            key={nextKey()}
            className="px-1 py-0.5 rounded bg-[#1A1A1E] text-[#4ADE80] font-mono text-[11px]"
          >
            {token.slice(1, -1)}
          </code>
        );
      } else if (token.startsWith("**") || token.startsWith("__")) {
        parts.push(
          <strong key={nextKey()} className="font-semibold text-[#F8F8FA]">
            {token.slice(2, -2)}
          </strong>
        );
      } else {
        parts.push(
          <em key={nextKey()} className="italic text-[#E5E6EA]">
            {token.slice(1, -1)}
          </em>
        );
      }
      last = m.index + token.length;
    }
    if (last < text.length) parts.push(text.slice(last));
    return <span>{parts}</span>;
  }

  while (i < lines.length) {
    const line = lines[i]!;

    // Fenced code block
    if (line.startsWith("```")) {
      const lang = line.slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i]!.startsWith("```")) {
        codeLines.push(lines[i]!);
        i++;
      }
      elements.push(
        <pre
          key={nextKey()}
          className="mt-2 mb-4 bg-[#1A1A1E] border border-[#33353A] rounded-md px-4 py-3 font-mono text-[11px] text-[#E5E6EA] leading-relaxed overflow-x-auto whitespace-pre"
        >
          {lang && (
            <span className="block text-[10px] text-[#C8CAD0] mb-2 uppercase tracking-wider">
              {lang}
            </span>
          )}
          {codeLines.join("\n")}
        </pre>
      );
      i++;
      continue;
    }

    // H1
    if (line.startsWith("# ")) {
      elements.push(
        <h1 key={nextKey()} className="text-[18px] font-semibold text-[#F8F8FA] mt-5 mb-2">
          {inlineFormat(line.slice(2))}
        </h1>
      );
      i++;
      continue;
    }

    // H2
    if (line.startsWith("## ")) {
      elements.push(
        <h2
          key={nextKey()}
          className="text-[14px] font-semibold text-[#F8F8FA] mt-5 mb-2 pb-1.5 border-b border-[#33353A]"
        >
          {inlineFormat(line.slice(3))}
        </h2>
      );
      i++;
      continue;
    }

    // H3
    if (line.startsWith("### ")) {
      elements.push(
        <h3 key={nextKey()} className="text-[13px] font-semibold text-[#F8F8FA] mt-4 mb-1.5">
          {inlineFormat(line.slice(4))}
        </h3>
      );
      i++;
      continue;
    }

    // Unordered list item
    if (/^[-*] /.test(line)) {
      const items: React.ReactElement[] = [];
      while (i < lines.length && /^[-*] /.test(lines[i]!)) {
        items.push(
          <li key={nextKey()} className="flex gap-2 text-[13px] text-[#E5E6EA] leading-relaxed">
            <span className="mt-1.5 w-1 h-1 rounded-full bg-[#C8CAD0] flex-shrink-0" />
            <span>{inlineFormat(lines[i]!.replace(/^[-*] /, ""))}</span>
          </li>
        );
        i++;
      }
      elements.push(
        <ul key={nextKey()} className="my-2 space-y-1.5 pl-1">
          {items}
        </ul>
      );
      continue;
    }

    // Ordered list item
    if (/^\d+\. /.test(line)) {
      const items: React.ReactElement[] = [];
      let n = 1;
      while (i < lines.length && /^\d+\. /.test(lines[i]!)) {
        items.push(
          <li key={nextKey()} className="flex gap-2.5 text-[13px] text-[#E5E6EA] leading-relaxed">
            <span className="flex-shrink-0 text-[11px] text-[#C8CAD0] font-mono w-4 text-right mt-0.5">
              {n}.
            </span>
            <span>{inlineFormat(lines[i]!.replace(/^\d+\. /, ""))}</span>
          </li>
        );
        i++;
        n++;
      }
      elements.push(
        <ol key={nextKey()} className="my-2 space-y-1.5">
          {items}
        </ol>
      );
      continue;
    }

    // Markdown table — detected by pipe-delimited line followed by separator row
    if (line.includes("|") && i + 1 < lines.length && /^\|?[\s|:-]+\|/.test(lines[i + 1]!)) {
      const parseRow = (row: string) =>
        row
          .replace(/^\|/, "")
          .replace(/\|$/, "")
          .split("|")
          .map((cell) => cell.trim());

      const headers = parseRow(line);
      i += 2; // skip header + separator

      const rows: string[][] = [];
      while (i < lines.length && lines[i]!.includes("|")) {
        rows.push(parseRow(lines[i]!));
        i++;
      }

      elements.push(
        <div key={nextKey()} className="my-4 overflow-x-auto">
          <table className="w-full text-[12px] border-collapse">
            <thead>
              <tr>
                {headers.map((h, hi) => (
                  <th
                    key={hi}
                    className="text-left px-3 py-2 text-[#F8F8FA] font-semibold border-b border-[#33353A] bg-[#1A1A1E] whitespace-nowrap"
                  >
                    {inlineFormat(h)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((cells, ri) => (
                <tr key={ri} className={ri % 2 === 0 ? "bg-[#222327]" : "bg-[#1E1F23]"}>
                  {cells.map((cell, ci) => (
                    <td
                      key={ci}
                      className="px-3 py-2 text-[#E5E6EA] border-b border-[#33353A]/40 align-top"
                    >
                      {inlineFormat(cell)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
      continue;
    }

    // Horizontal rule
    if (/^---+$/.test(line.trim())) {
      elements.push(<hr key={nextKey()} className="my-4 border-[#33353A]" />);
      i++;
      continue;
    }

    // Blank line
    if (line.trim() === "") {
      i++;
      continue;
    }

    // Paragraph
    elements.push(
      <p key={nextKey()} className="text-[13px] text-[#E5E6EA] leading-relaxed my-2">
        {inlineFormat(line)}
      </p>
    );
    i++;
  }

  return <>{elements}</>;
}

// ── Registry types (mirrors Rust SkillSource) ────────────────────────────────

interface SkillSource {
  source: string; // "owner/repo"
  id: string;     // "owner/repo/skill-name"
}

// ── Component ───────────────────────────────────────────────────────────────

export default function SkillStore({ resetKey }: { resetKey?: number }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<RemoteSkillResult[]>([]);
  const [selected, setSelected] = useState<RemoteSkillResult | null>(null);
  const [rawContent, setRawContent] = useState<string>("");
  const [searching, setSearching] = useState(false);
  const [loadingPreview, setLoadingPreview] = useState(false);
  // registry: skill name → SkillSource, loaded from ~/.automatic/skills.json
  const [registry, setRegistry] = useState<Record<string, SkillSource>>({});
  const [importing, setImporting] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [showDuplicates, setShowDuplicates] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Return to landing when the nav item is clicked again
  useEffect(() => {
    if (resetKey !== undefined) {
      setSelected(null);
      setRawContent("");
      setQuery("");
      setResults([]);
      setPreviewError(null);
      setSearchError(null);
    }
  }, [resetKey]);

  // ── Load registry on mount ───────────────────────────────────────────────

  const loadRegistry = useCallback(async () => {
    try {
      const raw: string = await invoke("get_skill_sources");
      setRegistry(JSON.parse(raw));
    } catch {
      // Non-fatal — just means no skills have been imported yet
    }
  }, []);

  useEffect(() => { loadRegistry(); }, [loadRegistry]);

  // ── Search ──────────────────────────────────────────────────────────────

  const doSearch = useCallback(async (q: string) => {
    if (q.trim().length < 2) {
      setResults([]);
      setSearchError(null);
      return;
    }
    setSearching(true);
    setSearchError(null);
    try {
      const res: RemoteSkillResult[] = await invoke("search_remote_skills", { query: q });
      setResults(res);
      setShowDuplicates(false);
    } catch (err: any) {
      setSearchError(`Search failed: ${err}`);
      setResults([]);
    } finally {
      setSearching(false);
    }
  }, []);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(query), 300);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query, doSearch]);

  // ── Select / deselect ───────────────────────────────────────────────────

  const handleSelect = useCallback(
    async (skill: RemoteSkillResult) => {
      if (selected?.id === skill.id) {
        setSelected(null);
        setRawContent("");
        setPreviewError(null);
        return;
      }

      setSelected(skill);
      setRawContent("");
      setPreviewError(null);
      setLoadingPreview(true);
      try {
        const content: string = await invoke("fetch_remote_skill_content", {
          source: skill.source,
          name: skill.name,
        });
        setRawContent(content);
      } catch (err: any) {
        setPreviewError(`Could not load preview: ${err}`);
      } finally {
        setLoadingPreview(false);
      }
    },
    [selected]
  );

  // ── Import ──────────────────────────────────────────────────────────────

  const importSkill = useCallback(async () => {
    if (!selected || !rawContent) return;
    setImporting(true);
    try {
      await invoke("import_remote_skill", {
        name: selected.name,
        content: rawContent,
        source: selected.source,
        id: selected.id,
      });
      trackSkillInstalled(selected.name, selected.source);
      // Update local registry mirror
      setRegistry((prev) => ({
        ...prev,
        [selected.name]: { source: selected.source, id: selected.id },
      }));
    } catch (err: any) {
      setPreviewError(`Import failed: ${err}`);
    } finally {
      setImporting(false);
    }
  }, [selected, rawContent]);

  // ── Update ───────────────────────────────────────────────────────────────
  // Re-fetches remote SKILL.md and overwrites the local copy.

  const updateSkill = useCallback(async () => {
    if (!selected || !rawContent) return;
    setUpdating(true);
    setPreviewError(null);
    try {
      // Re-fetch latest content first
      const fresh: string = await invoke("fetch_remote_skill_content", {
        source: selected.source,
        name: selected.name,
      });
      await invoke("import_remote_skill", {
        name: selected.name,
        content: fresh,
        source: selected.source,
        id: selected.id,
      });
      trackSkillUpdated(selected.name);
      setRawContent(fresh);
    } catch (err: any) {
      setPreviewError(`Update failed: ${err}`);
    } finally {
      setUpdating(false);
    }
  }, [selected, rawContent]);

  // ── Derived state ───────────────────────────────────────────────────────

  // A skill is "imported" if it appears in the registry with matching id
  const isImported = selected
    ? Object.values(registry).some((s) => s.id === selected.id)
    : false;

  const { meta, body } = rawContent ? parseFrontmatter(rawContent) : { meta: {}, body: "" };
  const displayName = meta.name || selected?.name || "";
  const description = meta.description || "";

  const searchInputRef = useRef<HTMLInputElement>(null);

  const SAMPLE_SEARCHES = [
    "laravel",
    "react best practices",
    "github workflow",
    "code review",
    "tailwind",
    "pair programming",
  ];

  // ── Render ──────────────────────────────────────────────────────────────

  // Landing page shown when there is no active query and no skill selected
  if (!query.trim() && !selected) {
    return (
      <div className="flex h-full flex-col overflow-y-auto custom-scrollbar bg-[#222327]">
        <div className="flex flex-col px-6 pt-10 pb-10 w-full">
          <div className="w-full max-w-2xl mx-auto">
            {/* Icon + heading */}
            <div className="text-center mb-6 max-w-lg mx-auto">
              <div className="w-12 h-12 mx-auto mb-4 rounded-2xl bg-[#4ADE80]/10 border border-[#4ADE80]/20 flex items-center justify-center">
                <Download size={20} className="text-[#4ADE80]" strokeWidth={1.5} />
              </div>
              <h2 className="text-[18px] font-semibold text-[#F8F8FA] mb-1.5">Skill Store</h2>
              <p className="text-[13px] text-[#C8CAD0] leading-relaxed">
                Browse and install agent skills from{" "}
                <a
                  href="https://skills.sh"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[#4ADE80] hover:text-[#6EE7A0] transition-colors"
                >
                  skills.sh
                </a>
                . Skills are reusable instruction sets that extend what your AI agents can do.
              </p>
            </div>

            {/* Big search box */}
            <div className="relative mb-3">
              <Search size={16} className="absolute left-4 top-1/2 -translate-y-1/2 text-[#C8CAD0] pointer-events-none" />
              <input
                ref={searchInputRef}
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search for skills…"
                autoFocus
                className="w-full bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#4ADE80] rounded-xl pl-11 pr-4 py-3 text-[14px] text-[#F8F8FA] placeholder-[#C8CAD0]/60 outline-none transition-colors shadow-sm"
              />
            </div>

            {/* Sample searches */}
            <div className="flex flex-wrap gap-2 justify-center mb-6">
              {SAMPLE_SEARCHES.map((s) => (
                <button
                  key={s}
                  onClick={() => setQuery(s)}
                  className="px-3 py-1.5 rounded-full bg-[#2D2E36] border border-[#33353A] text-[12px] text-[#C8CAD0] hover:text-[#F8F8FA] hover:border-[#44474F] transition-colors"
                >
                  {s}
                </button>
              ))}
            </div>

            {/* Featured skills */}
            <div>
              <h3 className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-3">
                Featured
              </h3>
              <div className="grid grid-cols-2 gap-3">
                {featuredSkillsData.map((skill) => {
                  const alreadyImported = Object.values(registry).some((s) => s.id === skill.id);
                  return (
                    <button
                      key={skill.id}
                      onClick={() => handleSelect({ id: skill.id, name: skill.name, source: skill.source, installs: skill.installs })}
                      className="group text-left p-4 rounded-xl bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] hover:bg-[#1E1F24] transition-colors"
                    >
                      <div className="flex items-start justify-between gap-2 mb-2">
                        <span className="text-[13px] font-medium text-[#F8F8FA] leading-snug">
                          {skill.displayName}
                        </span>
                        <div className="flex items-center gap-1.5 flex-shrink-0 mt-0.5">
                          {alreadyImported && (
                            <CheckCircle2 size={12} className="text-[#4ADE80]" />
                          )}
                          <ArrowRight size={12} className="text-[#33353A] group-hover:text-[#C8CAD0] transition-colors" />
                        </div>
                      </div>
                      <p className="text-[12px] text-[#C8CAD0] leading-relaxed mb-3 line-clamp-2">
                        {skill.description}
                      </p>
                      <div className="flex items-center gap-2">
                        <span className="text-[10px] text-[#C8CAD0]/60 truncate">{skill.source}</span>
                        <span className="text-[10px] text-[#C8CAD0]/30">·</span>
                        <span className="text-[10px] text-[#4ADE80] flex-shrink-0">
                          {formatInstalls(skill.installs)}
                        </span>
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full">
      {/* Left pane — search + results */}
      <div className="w-[264px] flex-shrink-0 border-r border-[#33353A] flex flex-col">
        {/* Back + search box */}
        <div className="p-3 border-b border-[#33353A]">
          <button
            onClick={() => { setQuery(""); setSelected(null); setRawContent(""); setPreviewError(null); setResults([]); }}
            className="flex items-center gap-1 text-[11px] text-[#C8CAD0] hover:text-[#F8F8FA] transition-colors mb-2"
          >
            <ArrowLeft size={11} />
            Skill Store
          </button>
          <div className="relative">
            <Search
              size={13}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[#C8CAD0] pointer-events-none"
            />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search skills.sh…"
              className="w-full bg-[#2D2E36] border border-[#33353A] rounded-md pl-8 pr-8 py-1.5 text-[13px] text-[#F8F8FA] placeholder-[#C8CAD0] focus:outline-none focus:border-[#4ADE80] transition-colors"
              autoFocus
            />
            {searching && (
              <Loader2
                size={13}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-[#C8CAD0] animate-spin"
              />
            )}
          </div>
          {searchError && <p className="mt-2 text-[11px] text-red-400">{searchError}</p>}
        </div>

        {/* Results list */}
        <div className="flex-1 overflow-y-auto custom-scrollbar">

          {results.length === 0 && !searching && query.trim().length >= 2 && !searchError && (
            <div className="p-4 text-center text-[13px] text-[#C8CAD0]">No skills found</div>
          )}
          {(() => {
            // Tag each result as duplicate or not (first occurrence of a name wins)
            const seenNames = new Set<string>();
            const tagged = results.map((skill) => {
              const isDuplicate = seenNames.has(skill.name);
              seenNames.add(skill.name);
              return { skill, isDuplicate };
            });
            const duplicateCount = tagged.filter((t) => t.isDuplicate).length;
            const visible = tagged.filter((t) => !t.isDuplicate || showDuplicates);

            return (
              <>
                {visible.map(({ skill, isDuplicate }) => {
                  const isActive = selected?.id === skill.id;
                  const alreadyImported = Object.values(registry).some((s) => s.id === skill.id);
                  return (
                    <button
                      key={skill.id}
                      onClick={() => handleSelect(skill)}
                      className={`w-full text-left px-3 py-2.5 border-b border-[#33353A]/40 transition-colors ${
                        isActive ? "bg-[#2D2E36]" : "hover:bg-[#2D2E36]/50"
                      } ${isDuplicate ? "opacity-50" : ""}`}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <span className={`text-[13px] font-medium leading-snug break-all ${isDuplicate ? "text-[#C8CAD0]" : "text-[#F8F8FA]"}`}>
                          {skill.name}
                        </span>
                        <div className="flex items-center gap-1 flex-shrink-0 mt-0.5">
                          {isDuplicate && (
                            <span className="text-[9px] text-[#C8CAD0]/70 bg-[#33353A] rounded px-1 py-[1px] leading-none">
                              duplicate
                            </span>
                          )}
                          {alreadyImported && (
                            <CheckCircle2 size={12} className="text-[#4ADE80]" />
                          )}
                        </div>
                      </div>
                      <div className="flex items-center gap-1.5 mt-0.5">
                        <span className="text-[11px] text-[#C8CAD0] truncate">{skill.source}</span>
                        <span className="text-[10px] text-[#C8CAD0]/40">·</span>
                        <span className="text-[11px] text-[#4ADE80] flex-shrink-0">
                          {formatInstalls(skill.installs)}
                        </span>
                      </div>
                    </button>
                  );
                })}
                {duplicateCount > 0 && (
                  <button
                    onClick={() => setShowDuplicates((v) => !v)}
                    className="w-full text-center px-3 py-2 text-[11px] text-[#C8CAD0]/70 hover:text-[#C8CAD0] transition-colors border-b border-[#33353A]/40"
                  >
                    {showDuplicates
                      ? "Hide duplicates"
                      : `${duplicateCount} likely duplicate${duplicateCount === 1 ? "" : "s"} hidden`}
                    <span className="ml-1.5 text-[10px] text-[#C8CAD0]/50">
                      {showDuplicates ? "▲" : "▼"}
                    </span>
                  </button>
                )}
              </>
            );
          })()}
        </div>
      </div>

      {/* Right pane — preview */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {!selected ? (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center">
              <div className="w-12 h-12 rounded-full border-2 border-dashed border-[#33353A] flex items-center justify-center mx-auto mb-4">
                <Download size={18} className="text-[#C8CAD0]" />
              </div>
              <h3 className="text-[14px] font-medium text-[#F8F8FA] mb-1">
                Select a skill to preview
              </h3>
              <p className="text-[13px] text-[#C8CAD0]">
                Search above and click a result
              </p>
            </div>
          </div>
        ) : (
          <div className="flex-1 overflow-y-auto custom-scrollbar">
            {loadingPreview ? (
              <div className="flex items-center gap-2 p-6 text-[13px] text-[#C8CAD0]">
                <Loader2 size={14} className="animate-spin" />
                Loading SKILL.md…
              </div>
            ) : previewError ? (
              <div className="p-8 max-w-lg">
                {/* Back button */}
                <button
                  onClick={() => { setSelected(null); setRawContent(""); setPreviewError(null); }}
                  className="flex items-center gap-1 text-[11px] text-[#C8CAD0] hover:text-[#F8F8FA] transition-colors mb-6"
                >
                  <ArrowLeft size={11} />
                  Back
                </button>

                <h3 className="text-[15px] font-medium text-[#F8F8FA] mb-2">
                  Skill not found in repository
                </h3>
                <p className="text-[13px] text-[#C8CAD0] leading-relaxed mb-5">
                  This skill does not conform to any known skill directory structure.
                  It may have been removed from the repository, or stored in an
                  unsupported layout.
                </p>

                {selected && (
                  <div className="rounded-lg border border-[#33353A] bg-[#2D2E36]/50 p-4 mb-5">
                    <div className="grid gap-2.5 text-[12px]">
                      <div className="flex items-baseline gap-2">
                        <span className="text-[#C8CAD0]/60 w-16 flex-shrink-0">Skill</span>
                        <span className="text-[#F8F8FA] font-medium break-all">{selected.name}</span>
                      </div>
                      <div className="flex items-baseline gap-2">
                        <span className="text-[#C8CAD0]/60 w-16 flex-shrink-0">Source</span>
                        <span className="text-[#C8CAD0] break-all">{selected.source}</span>
                      </div>
                      <div className="flex items-baseline gap-2">
                        <span className="text-[#C8CAD0]/60 w-16 flex-shrink-0">ID</span>
                        <span className="text-[#C8CAD0]/80 break-all font-mono text-[11px]">{selected.id}</span>
                      </div>
                    </div>
                  </div>
                )}

                {selected && (
                  <div className="flex flex-wrap gap-2">
                    <a
                      href={`https://skills.sh/${selected.id}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="inline-flex items-center gap-1.5 text-[12px] text-[#C8CAD0] hover:text-[#F8F8FA] bg-[#33353A] hover:bg-[#3D3F46] rounded-md px-3 py-1.5 transition-colors"
                    >
                      <ExternalLink size={11} />
                      View on skills.sh
                    </a>
                    <a
                      href={`https://github.com/${selected.source}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="inline-flex items-center gap-1.5 text-[12px] text-[#C8CAD0] hover:text-[#F8F8FA] bg-[#33353A] hover:bg-[#3D3F46] rounded-md px-3 py-1.5 transition-colors"
                    >
                      <Github size={11} />
                      Browse repository
                    </a>
                  </div>
                )}
              </div>
            ) : (
              <div className="flex min-h-full">
                {/* Main content column */}
                <div className="flex-1 min-w-0 px-8 py-6">
                  {/* Breadcrumb */}
                  <div className="flex items-center gap-1.5 text-[11px] text-[#C8CAD0] mb-4">
                    <button
                      onClick={() => { setSelected(null); setRawContent(""); setPreviewError(null); }}
                      className="flex items-center gap-1 hover:text-[#F8F8FA] transition-colors"
                    >
                      <ArrowLeft size={11} />
                      Back
                    </button>
                    <span>/</span>
                    <a
                      href="https://skills.sh"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="hover:text-[#F8F8FA] transition-colors"
                    >
                      skills
                    </a>
                    <span>/</span>
                    <a
                      href={`https://github.com/${selected.source}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="hover:text-[#F8F8FA] transition-colors"
                    >
                      {selected.source}
                    </a>
                    <span>/</span>
                    <span className="text-[#F8F8FA]">{selected.name}</span>
                  </div>

                  {/* Title + install command */}
                  <h1 className="text-[22px] font-semibold text-[#F8F8FA] mb-3 leading-tight">
                    {displayName}
                  </h1>

                  <div className="flex items-center gap-2 mb-5">
                    <code className="flex-1 bg-[#1A1A1E] border border-[#33353A] rounded-md px-3 py-2 font-mono text-[11px] text-[#C8CAD0] truncate">
                      $ npx skills add {selected.source} --skill {selected.name}
                    </code>
                    {isImported ? (
                      <button
                        onClick={updateSkill}
                        disabled={updating || !rawContent}
                        className="flex items-center gap-1.5 px-3 py-2 rounded-md text-[12px] font-medium transition-colors flex-shrink-0 bg-[#2D2E36] border border-[#33353A] hover:border-[#4ADE80] text-[#F8F8FA] disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        {updating ? (
                          <>
                            <Loader2 size={12} className="animate-spin" />
                            Updating…
                          </>
                        ) : (
                          <>
                            <CheckCircle2 size={12} className="text-[#4ADE80]" />
                            Update
                          </>
                        )}
                      </button>
                    ) : (
                      <button
                        onClick={importSkill}
                        disabled={importing || !rawContent}
                        className="flex items-center gap-1.5 px-3 py-2 rounded-md text-[12px] font-medium transition-colors flex-shrink-0 bg-[#4ADE80] hover:bg-[#6EE7A0] text-[#1A1A1E] disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        {importing ? (
                          <>
                            <Loader2 size={12} className="animate-spin" />
                            Importing…
                          </>
                        ) : (
                          <>
                            <Download size={12} />
                            Import
                          </>
                        )}
                      </button>
                    )}
                  </div>

                  {/* Description from frontmatter */}
                  {description && (
                    <p className="text-[13px] text-[#C8CAD0] leading-relaxed mb-6 pb-6 border-b border-[#33353A]">
                      {description}
                    </p>
                  )}

                  {/* Rendered body */}
                  <div>{body && renderMarkdown(body)}</div>
                </div>

                {/* Sidebar — meta */}
                <div className="w-[180px] flex-shrink-0 border-l border-[#33353A] px-5 py-6 space-y-6">
                  {/* Installs */}
                  <div>
                    <div className="text-[10px] font-semibold text-[#C8CAD0] uppercase tracking-wider mb-1">
                      Weekly Installs
                    </div>
                    <div className="text-[20px] font-semibold text-[#F8F8FA]">
                      {formatInstalls(selected.installs)}
                    </div>
                  </div>

                  {/* Repository */}
                  <div>
                    <div className="text-[10px] font-semibold text-[#C8CAD0] uppercase tracking-wider mb-1.5">
                      Repository
                    </div>
                    <a
                      href={`https://github.com/${selected.source}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="flex items-center gap-1.5 text-[12px] text-[#4ADE80] hover:text-[#6EE7A0] transition-colors"
                    >
                      <Github size={12} className="flex-shrink-0" />
                      <span className="truncate">{selected.source}</span>
                    </a>
                  </div>

                  {/* skills.sh page */}
                  <div>
                    <div className="text-[10px] font-semibold text-[#C8CAD0] uppercase tracking-wider mb-1.5">
                      skills.sh
                    </div>
                    <a
                      href={`https://skills.sh/${selected.id}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="flex items-center gap-1.5 text-[12px] text-[#4ADE80] hover:text-[#6EE7A0] transition-colors"
                    >
                      <ExternalLink size={12} className="flex-shrink-0" />
                      <span className="truncate">View page</span>
                    </a>
                  </div>

                  {/* Skill name from frontmatter if different */}
                  {meta.name && meta.name !== selected.name && (
                    <div>
                      <div className="text-[10px] font-semibold text-[#C8CAD0] uppercase tracking-wider mb-1">
                        Skill ID
                      </div>
                      <div className="flex items-center gap-1.5 text-[12px] text-[#E5E6EA]">
                        <Package size={12} className="flex-shrink-0 text-[#C8CAD0]" />
                        <span className="truncate">{selected.name}</span>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
