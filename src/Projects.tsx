import { useState, useEffect, useRef, useCallback } from "react";
import { SkillSelector } from "./SkillSelector";
import { AgentSelector } from "./AgentSelector";
import { AgentIcon } from "./AgentIcon";
import { McpSelector } from "./McpSelector";
import { MarkdownPreview } from "./MarkdownPreview";
import { useCurrentUser } from "./ProfileContext";
import { invoke } from "@tauri-apps/api/core";
import { open, ask } from "@tauri-apps/plugin-dialog";
import {
  trackProjectCreated,
  trackProjectUpdated,
  trackProjectDeleted,
  trackProjectSynced,
  trackProjectAgentAdded,
  trackProjectAgentRemoved,
  trackProjectSkillAdded,
  trackProjectSkillRemoved,
  trackProjectMcpServerAdded,
  trackProjectMcpServerRemoved,
} from "./analytics";

import {
  Plus,
  X,
  FolderOpen,
  Check,
  Code,
  Server,
  Trash2,
  Bot,
  RefreshCw,
  FileText,
  LayoutTemplate,
  Edit2,
  Upload,
  ArrowRightLeft,
  GripVertical,
  Package,
  AlertCircle,
  ArrowRight,
  ScrollText,
  Files,
  SplitSquareHorizontal,
  Brain,
  RotateCcw,
  ChevronDown,
  Copy,
} from "lucide-react";

interface Project {
  name: string;
  description: string;
  directory: string;
  skills: string[];
  local_skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  created_at: string;
  updated_at: string;
  created_by?: string;
  file_rules?: Record<string, string[]>;
  instruction_mode?: string;
}

interface AgentInfo {
  id: string;
  label: string;
  description: string;
  /** Non-null when this agent cannot have MCP config written by Automatic. */
  mcp_note: string | null;
}

interface DriftedFile {
  path: string;
  reason: "missing" | "modified" | "stale" | "unreadable";
  /** Content Automatic would generate. Present only when reason === "modified". */
  expected?: string;
  /** Content currently on disk. Present only when reason === "modified". */
  actual?: string;
}

interface AgentDrift {
  agent_id: string;
  agent_label: string;
  files: DriftedFile[];
}

interface DriftReport {
  drifted: boolean;
  agents: AgentDrift[];
}

interface ProjectFileInfo {
  filename: string;
  agents: string[];
  exists: boolean;
  target_files?: string[];
}

interface ProjectTemplate {
  name: string;
  description: string;
  skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  unified_instruction?: string;
  unified_rules?: string[];
}

const SIDEBAR_MIN = 160;
const SIDEBAR_MAX = 400;
const SIDEBAR_DEFAULT = 192; // w-48 equivalent

// ── Skill frontmatter helpers (shared with Skills.tsx logic) ─────────────────

interface LocalSkillFrontmatter {
  name?: string;
  description?: string;
  [key: string]: string | undefined;
}

function parseLocalSkillFrontmatter(raw: string): { meta: LocalSkillFrontmatter; body: string } {
  const match = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!match) return { meta: {}, body: raw };

  const meta: LocalSkillFrontmatter = {};
  const lines = match[1]!.split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i]!;
    const colonIdx = line.indexOf(":");
    if (colonIdx === -1) { i++; continue; }
    const key = line.slice(0, colonIdx).trim();
    const rest = line.slice(colonIdx + 1).trim();
    if (!key) { i++; continue; }
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

const LOCAL_SKILL_XML_TAG_RE = /<[^>]+>/;
const LOCAL_SKILL_RESERVED_WORDS = ["anthropic", "claude"];
const LOCAL_SKILL_NAME_CHARSET_RE = /^[a-z0-9-]*$/;

function validateLocalSkillName(value: string): string | null {
  if (!value) return "Name is required.";
  if (value.length > 64) return "Name must be 64 characters or fewer.";
  if (!LOCAL_SKILL_NAME_CHARSET_RE.test(value)) return "Name may only contain lowercase letters, numbers, and hyphens.";
  if (LOCAL_SKILL_XML_TAG_RE.test(value)) return "Name must not contain XML tags.";
  for (const word of LOCAL_SKILL_RESERVED_WORDS) {
    if (value === word || value.startsWith(word + "-") || value.endsWith("-" + word) || value.includes("-" + word + "-")) {
      return `Name must not contain the reserved word "${word}".`;
    }
  }
  return null;
}

function validateLocalSkillDescription(value: string): string | null {
  if (!value.trim()) return "Description is required.";
  if (value.length > 1024) return "Description must be 1024 characters or fewer.";
  if (LOCAL_SKILL_XML_TAG_RE.test(value)) return "Description must not contain XML tags.";
  return null;
}

function buildLocalSkillFrontmatter(name: string, description: string): string {
  const safeDesc = description.includes(":") ? `"${description.replace(/"/g, '\\"')}"` : description;
  return `---\nname: ${name}\ndescription: ${safeDesc}\n---\n`;
}

// ─────────────────────────────────────────────────────────────────────────────

function emptyProject(name: string): Project {
  return {
    name,
    description: "",
    directory: "",
    skills: [],
    local_skills: [],
    mcp_servers: [],
    providers: [],
    agents: [],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    file_rules: {},
    instruction_mode: "per-agent",
  };
}

interface ProjectsProps {
  initialProject?: string | null;
  onInitialProjectConsumed?: () => void;
}

/**
 * Editor icon component.
 *
 * When `iconPath` is provided (a local filesystem path returned by the
 * `get_editor_icon` Tauri command) it renders the real app bundle icon as a
 * data URI returned by the `get_editor_icon` Tauri command.  Otherwise it
 * falls back to inline SVG approximations so the UI is never blank.
 *
 * SVG fallbacks sourced from the opencode project (MIT licence):
 * https://github.com/anomalyco/opencode/tree/dev/packages/ui/src/assets/icons/app
 */
function EditorIcon({ id, iconPath }: { id: string; iconPath?: string }) {
  const cls = "w-[16px] h-[16px] flex-shrink-0 rounded-[3px]";

  // iconPath is a "data:image/png;base64,..." URI from the Rust backend
  if (iconPath) {
    return (
      <img
        src={iconPath}
        alt={id}
        className={cls}
        style={{ objectFit: "contain" }}
      />
    );
  }
  switch (id) {
    case "finder":
      // macOS Finder — blue face icon
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="finder-bg" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#6AC4F9"/>
              <stop offset="100%" stopColor="#2176D9"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" fill="url(#finder-bg)"/>
          {/* Happy face - left half */}
          <ellipse cx="21" cy="32" rx="16" ry="22" fill="#E8F4FF"/>
          {/* Smiling face - right half */}
          <ellipse cx="43" cy="32" rx="16" ry="22" fill="#FFFFFF"/>
          {/* Eyes */}
          <circle cx="17" cy="26" r="4" fill="#2176D9"/>
          <circle cx="47" cy="26" r="4" fill="#48AFEE"/>
          <circle cx="16" cy="25" r="1.5" fill="white"/>
          <circle cx="46" cy="25" r="1.5" fill="white"/>
          {/* Smile */}
          <path d="M28 40 Q32 46 36 40" stroke="#555" strokeWidth="2" fill="none" strokeLinecap="round"/>
          {/* Nose */}
          <ellipse cx="32" cy="35" rx="2" ry="1.5" fill="#DDD"/>
        </svg>
      );
    case "vscode":
      // Real VS Code icon from opencode (MIT)
      return (
        <svg className={cls} xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 100 100">
          <mask id="vscode-a" width="100" height="100" x="0" y="0" maskUnits="userSpaceOnUse" style={{maskType:"alpha"}}>
            <path fill="#fff" fillRule="evenodd" d="M70.912 99.317a6.223 6.223 0 0 0 4.96-.19l20.589-9.907A6.25 6.25 0 0 0 100 83.587V16.413a6.25 6.25 0 0 0-3.54-5.632L75.874.874a6.226 6.226 0 0 0-7.104 1.21L29.355 38.04 12.187 25.01a4.162 4.162 0 0 0-5.318.236l-5.506 5.009a4.168 4.168 0 0 0-.004 6.162L16.247 50 1.36 63.583a4.168 4.168 0 0 0 .004 6.162l5.506 5.01a4.162 4.162 0 0 0 5.318.236l17.168-13.032L68.77 97.917a6.217 6.217 0 0 0 2.143 1.4ZM75.015 27.3 45.11 50l29.906 22.701V27.3Z" clipRule="evenodd"/>
          </mask>
          <g mask="url(#vscode-a)">
            <path fill="#0065A9" d="M96.461 10.796 75.857.876a6.23 6.23 0 0 0-7.107 1.207l-67.451 61.5a4.167 4.167 0 0 0 .004 6.162l5.51 5.009a4.167 4.167 0 0 0 5.32.236l81.228-61.62c2.725-2.067 6.639-.124 6.639 3.297v-.24a6.25 6.25 0 0 0-3.539-5.63Z"/>
            <path fill="#007ACC" d="m96.461 89.204-20.604 9.92a6.229 6.229 0 0 1-7.107-1.207l-67.451-61.5a4.167 4.167 0 0 1 .004-6.162l5.51-5.009a4.167 4.167 0 0 1 5.32-.236l81.228 61.62c2.725 2.067 6.639.124 6.639-3.297v.24a6.25 6.25 0 0 1-3.539 5.63Z"/>
            <path fill="#1F9CF0" d="M75.858 99.126a6.232 6.232 0 0 1-7.108-1.21c2.306 2.307 6.25.674 6.25-2.588V4.672c0-3.262-3.944-4.895-6.25-2.589a6.232 6.232 0 0 1 7.108-1.21l20.6 9.908A6.25 6.25 0 0 1 100 16.413v67.174a6.25 6.25 0 0 1-3.541 5.633l-20.601 9.906Z"/>
          </g>
        </svg>
      );
    case "cursor":
      // Real Cursor icon from opencode (MIT)
      return (
        <svg className={cls} fill="none" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
          <rect width="512" height="512" rx="122" fill="#000"/>
          <g clipPath="url(#cursor-clip)">
            <mask id="cursor-mask" style={{maskType:"luminance"}} maskUnits="userSpaceOnUse" x="85" y="89" width="343" height="334">
              <path d="M85 89h343v334H85V89z" fill="#fff"/>
            </mask>
            <g mask="url(#cursor-mask)">
              <path d="M255.428 423l148.991-83.5L255.428 256l-148.99 83.5 148.99 83.5z" fill="url(#cursor-g0)"/>
              <path d="M404.419 339.5v-167L255.428 89v167l148.991 83.5z" fill="url(#cursor-g1)"/>
              <path d="M255.428 89l-148.99 83.5v167l148.99-83.5V89z" fill="url(#cursor-g2)"/>
              <path d="M404.419 172.5L255.428 423V256l148.991-83.5z" fill="#E4E4E4"/>
              <path d="M404.419 172.5L255.428 256l-148.99-83.5h297.981z" fill="#fff"/>
            </g>
          </g>
          <defs>
            <linearGradient id="cursor-g0" x1="255.428" y1="256" x2="255.428" y2="423" gradientUnits="userSpaceOnUse">
              <stop offset=".16" stopColor="#fff" stopOpacity=".39"/>
              <stop offset=".658" stopColor="#fff" stopOpacity=".8"/>
            </linearGradient>
            <linearGradient id="cursor-g1" x1="404.419" y1="173.015" x2="257.482" y2="261.497" gradientUnits="userSpaceOnUse">
              <stop offset=".182" stopColor="#fff" stopOpacity=".31"/>
              <stop offset=".715" stopColor="#fff" stopOpacity="0"/>
            </linearGradient>
            <linearGradient id="cursor-g2" x1="255.428" y1="89" x2="112.292" y2="342.802" gradientUnits="userSpaceOnUse">
              <stop stopColor="#fff" stopOpacity=".6"/>
              <stop offset=".667" stopColor="#fff" stopOpacity=".22"/>
            </linearGradient>
            <clipPath id="cursor-clip">
              <path fill="#fff" transform="translate(85 89)" d="M0 0h343v334H0z"/>
            </clipPath>
          </defs>
        </svg>
      );
    case "zed":
      // Real Zed icon from opencode (MIT) — white on dark background
      return (
        <svg className={cls} xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96" fill="none">
          <rect width="96" height="96" rx="18" fill="#084CCE"/>
          <g clipPath="url(#zed-clip)">
            <path fill="#fff" fillRule="evenodd" d="M9 6a3 3 0 0 0-3 3v66H0V9a9 9 0 0 1 9-9h80.379c4.009 0 6.016 4.847 3.182 7.682L43.055 57.187H57V51h6v7.688a4.5 4.5 0 0 1-4.5 4.5H37.055L26.743 73.5H73.5V36h6v37.5a6 6 0 0 1-6 6H20.743L10.243 90H87a3 3 0 0 0 3-3V21h6v66a9 9 0 0 1-9 9H6.621c-4.009 0-6.016-4.847-3.182-7.682L52.757 39H39v6h-6v-7.5a4.5 4.5 0 0 1 4.5-4.5h21.257l10.5-10.5H22.5V60h-6V22.5a6 6 0 0 1 6-6h52.757L85.757 6H9Z" clipRule="evenodd"/>
          </g>
          <defs>
            <clipPath id="zed-clip"><path fill="#fff" d="M0 0h96v96H0z"/></clipPath>
          </defs>
        </svg>
      );
    case "textmate":
      // TextMate — ball-in-circle logo style, golden/dark
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect width="64" height="64" rx="14" fill="#1C1C1C"/>
          <circle cx="32" cy="32" r="20" fill="#2A2A2A" stroke="#4A4A4A" strokeWidth="1.5"/>
          <circle cx="32" cy="32" r="13" fill="#E8A820"/>
          <circle cx="32" cy="32" r="8" fill="#C88A10"/>
          <circle cx="28" cy="28" r="3" fill="#FFCF50"/>
          <text x="32" y="56" textAnchor="middle" fontSize="10" fontWeight="700" fill="#888" fontFamily="monospace">TM</text>
        </svg>
      );
    case "antigravity":
      // Real Antigravity icon from opencode (MIT)
      return (
        <svg className={cls} viewBox="0 0 16 15" fill="none" xmlns="http://www.w3.org/2000/svg">
          <mask id="ag-mask" style={{maskType:"alpha"}} maskUnits="userSpaceOnUse" x="0" y="0" width="16" height="15">
            <path d="M14.0777 13.984C14.945 14.6345 16.2458 14.2008 15.0533 13.0084C11.476 9.53949 12.2349 0 7.79033 0C3.34579 0 4.10461 9.53949 0.527295 13.0084C-0.773543 14.3092 0.635692 14.6345 1.50293 13.984C4.86344 11.7076 4.64663 7.69664 7.79033 7.69664C10.934 7.69664 10.7172 11.7076 14.0777 13.984Z" fill="black"/>
          </mask>
          <g mask="url(#ag-mask)">
            <g filter="url(#ag-f0)"><path d="-0.658907 -3.2306C-0.922679 -0.906781 1.07986 1.22861 3.81388 1.53894C6.54791 1.84927 8.97811 0.217009 9.24188 -2.10681C9.50565 -4.43063 7.50312 -6.56602 4.76909 -6.87635C2.03506 -7.18667 -0.395135 -5.55442 -0.658907 -3.2306Z" fill="#FFE432"/></g>
            <g filter="url(#ag-f1)"><path d="M9.88233 4.36642C10.5673 7.31568 13.566 9.13902 16.5801 8.43896C19.5942 7.73891 21.4823 4.78056 20.7973 1.83131C20.1123 -1.11795 17.1136 -2.94128 14.0995 -2.24123C11.0854 -1.54118 9.19733 1.41717 9.88233 4.36642Z" fill="#FC413D"/></g>
            <g filter="url(#ag-f2)"><path d="M-8.05291 6.34512C-7.18736 9.38883 -3.28925 10.9473 0.653774 9.82598C4.5968 8.7047 7.09158 5.32829 6.22603 2.28458C5.36048 -0.759142 1.46236 -2.31758 -2.48066 -1.19629C-6.42368 -0.0750048 -8.91846 3.3014 -8.05291 6.34512Z" fill="#00B95C"/></g>
            <g filter="url(#ag-f3)"><path d="M6.42819 17.2263C7.10197 20.1273 9.91278 21.953 12.7063 21.3042C15.4998 20.6553 17.2182 17.7777 16.5444 14.8767C15.8707 11.9757 13.0599 10.15 10.2663 10.7988C7.47281 11.4477 5.75441 14.3253 6.42819 17.2263Z" fill="#3186FF"/></g>
          </g>
          <defs>
            <filter id="ag-f0" x="-2.13" y="-8.36" width="12.84" height="11.38" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB"><feFlood floodOpacity="0" result="BackgroundImageFix"/><feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="0.72" result="effect1_foregroundBlur"/></filter>
            <filter id="ag-f1" x="2.75" y="-9.38" width="25.18" height="24.96" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB"><feFlood floodOpacity="0" result="BackgroundImageFix"/><feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="3.5" result="effect1_foregroundBlur"/></filter>
            <filter id="ag-f2" x="-14.17" y="-7.5" width="26.51" height="23.63" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB"><feFlood floodOpacity="0" result="BackgroundImageFix"/><feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="2.97" result="effect1_foregroundBlur"/></filter>
            <filter id="ag-f3" x="0.63" y="5.02" width="21.7" height="22.06" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB"><feFlood floodOpacity="0" result="BackgroundImageFix"/><feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="2.82" result="effect1_foregroundBlur"/></filter>
          </defs>
        </svg>
      );
    case "xcode":
      // Xcode — hammer + wrench on blue gradient background
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="xcode-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#3A8EFF"/>
              <stop offset="100%" stopColor="#0F5FD8"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#xcode-bg)"/>
          {/* Hammer handle */}
          <rect x="38" y="36" width="6" height="18" rx="2" transform="rotate(-45 38 36)" fill="#E8E8E8"/>
          {/* Hammer head */}
          <rect x="22" y="12" width="18" height="12" rx="3" fill="white"/>
          {/* Wrench */}
          <circle cx="42" cy="20" r="8" fill="none" stroke="#B0C8FF" strokeWidth="4"/>
          <rect x="40" y="24" width="4" height="18" rx="2" fill="#B0C8FF"/>
        </svg>
      );
    default:
      return <FolderOpen size={14} className="text-text-muted" />;
  }
}

// ── DriftDiffModal ────────────────────────────────────────────────────────────

interface DiffLine {
  type: "same" | "added" | "removed";
  content: string;
  lineNo: { a: number | null; b: number | null };
}

/** Compute a simple line-level diff between two text strings.
 *  Uses a greedy longest-common-subsequence approach suitable for config files. */
function computeLineDiff(expected: string, actual: string): DiffLine[] {
  const aLines = expected.split("\n");
  const bLines = actual.split("\n");

  // Build LCS table
  const m = aLines.length;
  const n = bLines.length;
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      if (aLines[i] === bLines[j]) {
        dp[i]![j] = 1 + dp[i + 1]![j + 1]!;
      } else {
        dp[i]![j] = Math.max(dp[i + 1]![j]!, dp[i]![j + 1]!);
      }
    }
  }

  const result: DiffLine[] = [];
  let i = 0, j = 0;
  let lineA = 1, lineB = 1;

  while (i < m || j < n) {
    if (i < m && j < n && aLines[i] === bLines[j]) {
      result.push({ type: "same", content: aLines[i]!, lineNo: { a: lineA++, b: lineB++ } });
      i++; j++;
    } else if (j < n && (i >= m || dp[i]![j + 1]! >= dp[i + 1]![j]!)) {
      result.push({ type: "added", content: bLines[j]!, lineNo: { a: null, b: lineB++ } });
      j++;
    } else {
      result.push({ type: "removed", content: aLines[i]!, lineNo: { a: lineA++, b: null } });
      i++;
    }
  }

  return result;
}

interface DriftDiffModalProps {
  file: DriftedFile;
  agentLabel: string;
  onClose: () => void;
}

function DriftDiffModal({ file, agentLabel, onClose }: DriftDiffModalProps) {
  const diffLines = file.expected != null && file.actual != null
    ? computeLineDiff(file.expected, file.actual)
    : null;

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border-strong/40 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: "min(900px, 90vw)", maxHeight: "80vh" }}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-border-strong flex-shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-[11px] font-medium text-warning/70 uppercase tracking-wider">{agentLabel}</span>
            <span className="text-border-strong">/</span>
            <span className="text-[13px] font-mono text-text-base">{file.path}</span>
            <span className={`text-[10px] font-medium px-1.5 py-0.5 rounded uppercase tracking-wider ${
              file.reason === "modified"
                ? "bg-warning/15 text-warning"
                : file.reason === "missing"
                ? "bg-danger/15 text-danger"
                : file.reason === "stale"
                ? "bg-text-muted/15 text-text-muted"
                : "bg-text-muted/15 text-text-muted"
            }`}>{file.reason}</span>
          </div>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text-base transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Legend */}
        {diffLines && (
          <div className="flex items-center gap-4 px-5 py-2 border-b border-border-strong flex-shrink-0 bg-bg-input">
            <div className="flex items-center gap-1.5">
              <div className="w-3 h-3 rounded-sm bg-success/20 border border-success/40" />
              <span className="text-[11px] text-text-muted">On disk (current)</span>
            </div>
            <div className="flex items-center gap-1.5">
              <div className="w-3 h-3 rounded-sm bg-danger/20 border border-danger/40" />
              <span className="text-[11px] text-text-muted">Automatic would generate (expected)</span>
            </div>
          </div>
        )}

        {/* Diff body */}
        <div className="overflow-auto flex-1 font-mono text-[12px]">
          {diffLines ? (
            <table className="w-full border-collapse">
              <tbody>
                {diffLines.map((line, idx) => (
                  <tr
                    key={idx}
                    className={
                      line.type === "added"
                        ? "bg-success/10 hover:bg-success/15"
                        : line.type === "removed"
                        ? "bg-danger/10 hover:bg-danger/15"
                        : "hover:bg-surface-hover"
                    }
                  >
                    {/* Line number: expected (a) */}
                    <td className="select-none text-right text-border-strong px-3 py-0.5 w-12 border-r border-border-strong min-w-[3rem]">
                      {line.lineNo.a ?? ""}
                    </td>
                    {/* Line number: actual (b) */}
                    <td className="select-none text-right text-border-strong px-3 py-0.5 w-12 border-r border-border-strong min-w-[3rem]">
                      {line.lineNo.b ?? ""}
                    </td>
                    {/* Sign */}
                    <td className={`select-none px-2 py-0.5 w-5 text-center font-bold ${
                      line.type === "added" ? "text-success" : line.type === "removed" ? "text-danger" : "text-border-strong"
                    }`}>
                      {line.type === "added" ? "+" : line.type === "removed" ? "−" : " "}
                    </td>
                    {/* Content */}
                    <td className={`px-3 py-0.5 whitespace-pre ${
                      line.type === "added"
                        ? "text-success"
                        : line.type === "removed"
                        ? "text-danger"
                        : "text-text-muted"
                    }`}>
                      {line.content}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            /* Non-modified reasons: nothing to diff — show a descriptive message */
            <div className="flex flex-col items-center justify-center h-full py-16 text-text-muted">
              {file.reason === "missing" && (
                <>
                  <p className="text-[13px] font-medium text-text-base mb-2">File is missing on disk</p>
                  <p className="text-[12px]">Automatic would create this file. Sync the project to resolve.</p>
                </>
              )}
              {file.reason === "stale" && (
                <>
                  <p className="text-[13px] font-medium text-text-base mb-2">Stale directory</p>
                  <p className="text-[12px]">This skill directory exists on disk but is no longer in the project config.</p>
                  <p className="text-[12px] mt-1">Sync the project to remove it.</p>
                </>
              )}
              {file.reason === "unreadable" && (
                <>
                  <p className="text-[13px] font-medium text-text-base mb-2">File could not be read</p>
                  <p className="text-[12px]">Check file permissions and try again.</p>
                </>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong flex-shrink-0">
          <button
            onClick={onClose}
            className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
          >
            Close
          </button>
        </div>
      </div>

    </div>
  );
}

export default function Projects({ initialProject = null, onInitialProjectConsumed }: ProjectsProps = {}) {
  const { userId } = useCurrentUser();
  const LAST_PROJECT_KEY = "automatic.projects.selected";
  const PROJECT_ORDER_KEY = "automatic.projects.order";

  // Migrate legacy "nexus." localStorage keys on first load
  useEffect(() => {
    const legacyKeys = [
      ["nexus.projects.selected", LAST_PROJECT_KEY],
      ["nexus.projects.order", PROJECT_ORDER_KEY],
    ];
    for (const [oldKey, newKey] of legacyKeys) {
      const val = localStorage.getItem(oldKey);
      if (val) {
        localStorage.setItem(newKey, val);
        localStorage.removeItem(oldKey);
      }
    }
  }, []);
  const [projects, setProjects] = useState<string[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(() => {
    return localStorage.getItem(LAST_PROJECT_KEY);
  });

  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT);
  const isSidebarDragging = useRef(false);

  const onSidebarMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isSidebarDragging.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, []);

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!isSidebarDragging.current) return;
      // 180px is the global sidebar width in App.tsx
      const newWidth = Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, e.clientX - 180));
      setSidebarWidth(newWidth);
    };
    const onMouseUp = () => {
      if (isSidebarDragging.current) {
        isSidebarDragging.current = false;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);

  // Drag-and-drop reorder state (pointer-events based)
  const [dragIdx, setDragIdx] = useState<number | null>(null);
  const [dropIdx, setDropIdx] = useState<number | null>(null);
  const dragIdxRef = useRef<number | null>(null);
  const dropIdxRef = useRef<number | null>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  // Wizard state (used while isCreating === true)
  const [wizardStep, setWizardStep] = useState<1 | 2 | 3>(1);
  const [wizardDiscovering, setWizardDiscovering] = useState(false);
  const [wizardDiscoveredAgents, setWizardDiscoveredAgents] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");

  // Available items to pick from
  const [availableAgents, setAvailableAgents] = useState<AgentInfo[]>([]);
  const [availableSkills, setAvailableSkills] = useState<string[]>([]);
  const [availableMcpServers, setAvailableMcpServers] = useState<string[]>([]);

  // Inline add state
  const [syncStatus, setSyncStatus] = useState<string | null>(null);

  // Drift detection state
  // null = unknown/not yet checked, DriftReport = result of last check
  const [driftReport, setDriftReport] = useState<DriftReport | null>(null);
  const driftCheckInFlight = useRef(false);

  // Per-project drift indicator: true = drifted, false = clean, undefined = unknown
  const [driftByProject, setDriftByProject] = useState<Record<string, boolean>>({});

  // Drift diff modal state — null when closed
  const [driftDiffFile, setDriftDiffFile] = useState<{ file: DriftedFile; agentLabel: string } | null>(null);

  // Project template state
  const [availableProjectTemplates, setAvailableProjectTemplates] = useState<ProjectTemplate[]>([]);
  const [showProjectTemplatePicker, setShowProjectTemplatePicker] = useState(false);
  const [selectedProjectTemplate, setSelectedProjectTemplate] = useState<string | null>(null);
  // Pending unified instruction content + rules to write after next save (from template apply)
  const pendingUnifiedInstruction = useRef<{ content: string; rules: string[] } | null>(null);

  // Project file state
  const [projectFiles, setProjectFiles] = useState<ProjectFileInfo[]>([]);
  const [activeProjectFile, setActiveProjectFile] = useState<string | null>(null);
  const [projectFileContent, setProjectFileContent] = useState("");
  const [projectFileEditing, setProjectFileEditing] = useState(false);
  const [projectFileDirty, setProjectFileDirty] = useState(false);
  const [projectFileSaving, setProjectFileSaving] = useState(false);
  const [availableTemplates, setAvailableTemplates] = useState<string[]>([]);
  const [showTemplatePicker, setShowTemplatePicker] = useState(false);
  const [availableRules, setAvailableRules] = useState<{ id: string; name: string }[]>([]);

  // Tab navigation within a project
  type ProjectTab = "summary" | "agents" | "skills" | "mcp_servers" | "project_file" | "memory";
  const [projectTab, setProjectTab] = useState<ProjectTab>("summary");

  // Memory state
  const [memories, setMemories] = useState<Record<string, { value: string; timestamp: string; source: string | null }>>({});
  const [loadingMemories, setLoadingMemories] = useState(false);
  const [editingMemoryKey, setEditingMemoryKey] = useState<string | null>(null);
  const [editingMemoryValue, setEditingMemoryValue] = useState<string>("");
  const [savingMemory, setSavingMemory] = useState(false);
  const [copiedMemoryKey, setCopiedMemoryKey] = useState<string | null>(null);

  // Local skill editing state
  const [localSkillEditing, setLocalSkillEditing] = useState<string | null>(null); // skill name being edited
  const [localSkillContent, setLocalSkillContent] = useState(""); // raw SKILL.md content
  const [localSkillEditName, setLocalSkillEditName] = useState("");
  const [localSkillEditDescription, setLocalSkillEditDescription] = useState("");
  const [localSkillEditBody, setLocalSkillEditBody] = useState("");
  const [localSkillFieldErrors, setLocalSkillFieldErrors] = useState<{ name: string | null; description: string | null }>({ name: null, description: null });
  const [localSkillIsEditing, setLocalSkillIsEditing] = useState(false);
  const [localSkillSaving, setLocalSkillSaving] = useState(false);

  // Editor detection state
  interface EditorInfo { id: string; label: string; installed: boolean; }
  const [installedEditors, setInstalledEditors] = useState<EditorInfo[]>([]);
  const [editorIconPaths, setEditorIconPaths] = useState<Record<string, string>>({});
  const [openInDropdownOpen, setOpenInDropdownOpen] = useState(false);
  const openInDropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadProjects();
    loadAvailableAgents();
    loadAvailableSkills();
    loadAvailableMcpServers();
    loadAvailableTemplates();
    loadAvailableRules();
    loadAvailableProjectTemplates();
    // Detect which editors are installed on this machine, then fetch real icons
    invoke<EditorInfo[]>("check_installed_editors").then((editors) => {
      setInstalledEditors(editors);
      // Request icon PNG paths for all known editors (not just installed ones —
      // we may want fallback icons for installed editors whose .icns is present
      // regardless of whether the CLI was found).
      const iconIds = editors.map((e) => e.id);
      Promise.all(
        iconIds.map((id) =>
          invoke<string>("get_editor_icon", { editorId: id })
            .then((path) => ({ id, path }))
            .catch(() => null)
        )
      ).then((results) => {
        const paths: Record<string, string> = {};
        for (const r of results) {
          if (r) paths[r.id] = r.path;
        }
        setEditorIconPaths(paths);
      });
    }).catch(() => {});
  }, []);

  // Close "Open in" dropdown when clicking outside
  useEffect(() => {
    if (!openInDropdownOpen) return;
    const handler = (e: MouseEvent) => {
      if (openInDropdownRef.current && !openInDropdownRef.current.contains(e.target as Node)) {
        setOpenInDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [openInDropdownOpen]);

  useEffect(() => {
    if (projects.length === 0) {
      return;
    }

    const preferred = selectedName && projects.includes(selectedName)
      ? selectedName
      : projects[0];

    if (preferred && (!project || project.name !== preferred) && !isCreating) {
      selectProject(preferred);
    }
  }, [projects]);

  // Navigate to a specific project when directed from another view (e.g. Agents)
  useEffect(() => {
    if (initialProject && projects.includes(initialProject)) {
      selectProject(initialProject);
      onInitialProjectConsumed?.();
    }
  }, [initialProject, projects]);

  // Reset drift state whenever the active project changes
  useEffect(() => {
    setDriftReport(null);
  }, [selectedName]);

  // Periodically check for configuration drift while a project tab is active
  useEffect(() => {
    const name = selectedName;
    if (!name || !project || !project.directory || project.agents.length === 0 || dirty || isCreating) {
      return;
    }

    const runCheck = async () => {
      if (driftCheckInFlight.current) return;
      driftCheckInFlight.current = true;
      try {
        const raw: string = await invoke("check_project_drift", { name });
        const report = JSON.parse(raw) as DriftReport;
        setDriftReport(report);
        setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
      } catch {
        // Silently ignore drift-check errors (e.g. directory gone)
      } finally {
        driftCheckInFlight.current = false;
      }
    };

    // Run immediately on mount / project change, then every 15 seconds
    runCheck();
    const interval = setInterval(runCheck, 15_000);
    return () => clearInterval(interval);
  }, [selectedName, project?.directory, project?.agents.length, dirty, isCreating]);

  // Background drift check for all projects (for sidebar indicators)
  useEffect(() => {
    if (projects.length === 0) return;

    let cancelled = false;
    const checkAll = async () => {
      // Stagger checks to avoid hammering the backend simultaneously
      for (const name of projects) {
        if (cancelled) return;
        try {
          const raw: string = await invoke("check_project_drift", { name });
          const report = JSON.parse(raw) as DriftReport;
          if (!cancelled) {
            setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
          }
        } catch {
          // Non-fatal — skip this project silently
        }
        // Small delay between each project to avoid UI jank
        await new Promise((res) => setTimeout(res, 200));
      }
    };

    checkAll();
    // Re-check all projects every 60 seconds
    const interval = setInterval(checkAll, 60_000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [projects]);

  const applyStoredOrder = (names: string[]): string[] => {
    try {
      const stored = localStorage.getItem(PROJECT_ORDER_KEY);
      if (!stored) return names.sort();
      const order: string[] = JSON.parse(stored);
      // Projects in stored order first, then any new ones alphabetically at the end
      const ordered: string[] = [];
      for (const n of order) {
        if (names.includes(n)) ordered.push(n);
      }
      const remaining = names.filter((n) => !ordered.includes(n)).sort();
      return [...ordered, ...remaining];
    } catch {
      return names.sort();
    }
  };

  const saveProjectOrder = (ordered: string[]) => {
    localStorage.setItem(PROJECT_ORDER_KEY, JSON.stringify(ordered));
  };

  // Compute which list index the pointer is over, skipping the "New Project" item
  const getDropIndex = (clientY: number): number | null => {
    if (!listRef.current) return null;
    const children = Array.from(listRef.current.children) as HTMLElement[];
    // If creating, the first child is the "New Project" placeholder — skip it
    const offset = isCreating ? 1 : 0;
    const items = children.slice(offset);
    for (let i = 0; i < items.length; i++) {
      const rect = items[i].getBoundingClientRect();
      const midY = rect.top + rect.height / 2;
      if (clientY < midY) return i;
    }
    return items.length;
  };

  const handleGripDown = (idx: number, e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragIdxRef.current = idx;
    dropIdxRef.current = idx;
    setDragIdx(idx);
    setDropIdx(idx);

    const onMove = (ev: PointerEvent) => {
      const target = getDropIndex(ev.clientY);
      dropIdxRef.current = target;
      setDropIdx(target);
    };

    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);

      const fromIdx = dragIdxRef.current;
      const toIdx = dropIdxRef.current;
      dragIdxRef.current = null;
      dropIdxRef.current = null;
      setDragIdx(null);
      setDropIdx(null);

      if (fromIdx === null || toIdx === null || fromIdx === toIdx) return;

      setProjects((prev) => {
        const reordered = [...prev];
        const [removed] = reordered.splice(fromIdx, 1);
        const insertAt = toIdx > fromIdx ? toIdx - 1 : toIdx;
        reordered.splice(insertAt, 0, removed);
        saveProjectOrder(reordered);
        return reordered;
      });
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  const loadProjects = async () => {
    try {
      const result: string[] = await invoke("get_projects");
      const ordered = applyStoredOrder(result);
      setProjects(ordered);
      setError(null);
    } catch (err: any) {
      setError(`Failed to load projects: ${err}`);
    }
  };

  const loadAvailableAgents = async () => {
    try {
      const result: AgentInfo[] = await invoke("list_agents");
      setAvailableAgents(result);
    } catch {
      // Agents list may not be available yet
    }
  };

  const loadAvailableSkills = async () => {
    try {
      const result: { name: string; in_agents: boolean; in_claude: boolean }[] = await invoke("get_skills");
      setAvailableSkills(result.map((e) => e.name).sort());
    } catch {
      // Skills may not exist yet
    }
  };

  const loadAvailableMcpServers = async () => {
    try {
      const result: string[] = await invoke("list_mcp_server_configs");
      setAvailableMcpServers(result.sort());
    } catch {
      // MCP servers may not exist yet
    }
  };

  const loadAvailableTemplates = async () => {
    try {
      const result: string[] = await invoke("get_templates");
      setAvailableTemplates(result.sort());
    } catch {
      // Templates may not exist yet
    }
  };

  const loadAvailableRules = async () => {
    try {
      const result: { id: string; name: string }[] = await invoke("get_rules");
      setAvailableRules(result.sort((a, b) => a.name.localeCompare(b.name)));
    } catch {
      // Rules may not exist yet
    }
  };

  const loadAvailableProjectTemplates = async () => {
    try {
      const names: string[] = await invoke("get_project_templates");
      const loaded: ProjectTemplate[] = await Promise.all(
        names.map(async (name) => {
          const raw: string = await invoke("read_project_template", { name });
          return JSON.parse(raw) as ProjectTemplate;
        })
      );
      setAvailableProjectTemplates(loaded);
    } catch {
      // Project templates may not exist yet
    }
  };

  const loadMemories = async (projectName: string) => {
    try {
      setLoadingMemories(true);
      const data: Record<string, { value: string; timestamp: string; source: string | null }> = await invoke("get_project_memories", { project: projectName });
      setMemories(data);
    } catch (err: any) {
      console.error("Failed to load memories:", err);
    } finally {
      setLoadingMemories(false);
    }
  };

  const applyProjectTemplate = (tmpl: ProjectTemplate) => {
    if (!project) return;
    // Merge: add template values, preserving anything already on the project
    const mergedAgents = [...new Set([...project.agents, ...tmpl.agents])];
    const mergedSkills = [...new Set([...project.skills, ...tmpl.skills])];
    const mergedMcpServers = [...new Set([...project.mcp_servers, ...tmpl.mcp_servers])];
    const mergedProviders = [...new Set([...project.providers, ...tmpl.providers])];
    const hasUnifiedContent = !!(tmpl.unified_instruction && tmpl.unified_instruction.trim());
    const hasUnifiedRules = (tmpl.unified_rules || []).length > 0;
    const hasUnified = hasUnifiedContent || hasUnifiedRules;
    setProject({
      ...project,
      description: project.description || tmpl.description,
      agents: mergedAgents,
      skills: mergedSkills,
      mcp_servers: mergedMcpServers,
      providers: mergedProviders,
      ...(hasUnified ? { instruction_mode: "unified" } : {}),
    });
    // Stash the unified instruction content + rules so handleSave can write them after the project is saved
    if (hasUnified) {
      pendingUnifiedInstruction.current = {
        content: tmpl.unified_instruction || "",
        rules: tmpl.unified_rules || [],
      };
    }
    setDirty(true);
    setSelectedProjectTemplate(tmpl.name);
    setShowProjectTemplatePicker(false);
  };

  const loadProjectFiles = async (name: string) => {
    try {
      const raw: string = await invoke("get_project_file_info", { name });
      const files: ProjectFileInfo[] = JSON.parse(raw);
      setProjectFiles(files);
      // Auto-select first file if none selected or previous one isn't available
      if (files.length > 0) {
        const currentValid = activeProjectFile && files.some(f => f.filename === activeProjectFile);
        const filename = currentValid ? activeProjectFile! : files[0].filename;
        setActiveProjectFile(filename);
        await loadProjectFileContent(name, filename);
      } else {
        setActiveProjectFile(null);
        setProjectFileContent("");
        setProjectFileEditing(false);
        setProjectFileDirty(false);
      }
    } catch {
      setProjectFiles([]);
      setActiveProjectFile(null);
      setProjectFileContent("");
    }
  };

  const loadProjectFileContent = async (projectName: string, filename: string) => {
    try {
      const content: string = await invoke("read_project_file", { name: projectName, filename });
      setProjectFileContent(content);
      setProjectFileEditing(false);
      setProjectFileDirty(false);
    } catch {
      setProjectFileContent("");
      setProjectFileEditing(false);
      setProjectFileDirty(false);
    }
  };

  const handleSaveProjectFile = async () => {
    if (!selectedName || !activeProjectFile) return;
    setProjectFileSaving(true);
    try {
      await invoke("save_project_file", {
        name: selectedName,
        filename: activeProjectFile,
        content: projectFileContent,
      });
      setProjectFileDirty(false);
    } catch (err: any) {
      setError(`Failed to save project file: ${err}`);
    } finally {
      setProjectFileSaving(false);
    }
  };

  const handleApplyTemplate = async (templateName: string) => {
    try {
      const content: string = await invoke("read_template", { name: templateName });
      setProjectFileContent(content);
      setProjectFileDirty(true);
      setProjectFileEditing(true);
      // Auto-enable Automatic rule if rules have never been configured for this file
      if (project && activeProjectFile) {
        const hasAutomatic = availableRules.some(r => r.id === "automatic-service");
        if (hasAutomatic && !project.file_rules?.[activeProjectFile]) {
          setProject({ ...project, file_rules: { ...(project.file_rules || {}), [activeProjectFile]: ["automatic-service"] } });
        }
      }
      setShowTemplatePicker(false);
    } catch (err: any) {
      setError(`Failed to load template: ${err}`);
    }
  };

  const selectProject = async (name: string) => {
    try {
      // Fetch both the stored state and the autodetected state in parallel so
      // we can tell whether detection found anything new that hasn't been saved.
      const [rawDetected, rawStored] = await Promise.all([
        invoke<string>("autodetect_project_dependencies", { name }),
        invoke<string>("read_project", { name }),
      ]);
      const parsed = JSON.parse(rawDetected);
      const stored = JSON.parse(rawStored);

      // Use stored config as the source of truth so that intentional user
      // removals (e.g. de-selecting an agent) are preserved. Autodetected
      // items are only merged in when they are genuinely new — i.e. present
      // in the detected result but absent from the stored config — never
      // added back once the user has removed them.
      const storedAgents: string[] = stored.agents || [];
      const storedSkills: string[] = stored.skills || [];
      const storedLocalSkills: string[] = stored.local_skills || [];
      const storedMcp: string[] = stored.mcp_servers || [];

      const detectedAgents: string[] = parsed.agents || [];
      const detectedSkills: string[] = parsed.skills || [];
      const detectedLocalSkills: string[] = parsed.local_skills || [];
      const detectedMcp: string[] = parsed.mcp_servers || [];

      // New items found by autodetect that aren't yet in the stored config.
      const newAgents = detectedAgents.filter((a) => !storedAgents.includes(a));
      const newSkills = detectedSkills.filter((s) => !storedSkills.includes(s));
      const newLocalSkills = detectedLocalSkills.filter((s) => !storedLocalSkills.includes(s));
      const newMcp = detectedMcp.filter((m) => !storedMcp.includes(m));

      const detectedDiffers =
        newAgents.length > 0 ||
        newSkills.length > 0 ||
        newLocalSkills.length > 0 ||
        newMcp.length > 0;

      // Normalize: ensure all fields exist with defaults for older projects.
      // Start from stored data and append any newly-detected items.
      const data: Project = {
        name: stored.name || name,
        description: stored.description || "",
        directory: stored.directory || "",
        skills: [...storedSkills, ...newSkills],
        local_skills: [...storedLocalSkills, ...newLocalSkills],
        mcp_servers: [...storedMcp, ...newMcp],
        providers: stored.providers || [],
        agents: [...storedAgents, ...newAgents],
        created_at: stored.created_at || new Date().toISOString(),
        updated_at: stored.updated_at || new Date().toISOString(),
        file_rules: stored.file_rules || {},
        instruction_mode: stored.instruction_mode || "per-agent",
      };

      setSelectedName(name);
      localStorage.setItem(LAST_PROJECT_KEY, name);
      setProject(data);
      setDirty(detectedDiffers);
      setIsCreating(false);
      setError(null);
      // Load project files for this project
      if (data.directory && data.agents.length > 0) {
        await loadProjectFiles(name);
      } else {
        setProjectFiles([]);
        setActiveProjectFile(null);
        setProjectFileContent("");
        setProjectFileEditing(false);
        setProjectFileDirty(false);
      }
      await loadMemories(name);
    } catch (err: any) {
      setError(`Failed to read project: ${err}`);
    }
  };

  const updateField = <K extends keyof Project>(
    key: K,
    value: Project[K]
  ) => {
    if (!project) return;
    setProject({ ...project, [key]: value });
    setDirty(true);
  };

  // Reload project state from disk and refresh all dependent UI
   const reloadProject = async (name: string) => {
    try {
      const raw: string = await invoke("read_project", { name });
      const parsed = JSON.parse(raw);
      const data: Project = {
        name: parsed.name || name,
        description: parsed.description || "",
        directory: parsed.directory || "",
        skills: parsed.skills || [],
        local_skills: parsed.local_skills || [],
        mcp_servers: parsed.mcp_servers || [],
        providers: parsed.providers || [],
        agents: parsed.agents || [],
        created_at: parsed.created_at || new Date().toISOString(),
        updated_at: parsed.updated_at || new Date().toISOString(),
        file_rules: parsed.file_rules || {},
        instruction_mode: parsed.instruction_mode || "per-agent",
      };
      setProject(data);
      setDirty(false);

      await loadAvailableSkills();
      await loadAvailableMcpServers();
      await loadMemories(name);

      if (data.directory && data.agents.length > 0) {
        await loadProjectFiles(name);
      } else {
        setProjectFiles([]);
        setActiveProjectFile(null);
        setProjectFileContent("");
        setProjectFileEditing(false);
        setProjectFileDirty(false);
      }
    } catch (err: any) {
      setError(`Failed to reload project: ${err}`);
    }
  };

  const handleSave = async () => {
    if (!project) return;
    const folderName = project.directory
      ? project.directory.split("/").filter(Boolean).pop() ?? ""
      : "";
    const name = isCreating
      ? (newName.trim() || folderName)
      : selectedName;
    if (!name) return;
    try {
      setSyncStatus("syncing");
      const toSave = { ...project, name, updated_at: new Date().toISOString() };
      // Tag new projects with the current user for future team/cloud sync
      if (isCreating && userId && !toSave.created_by) {
        toSave.created_by = userId;
      }
      // save_project writes the project config AND syncs all agent configs
      // (skills, MCP servers) in one atomic backend call.
      await invoke("save_project", {
        name,
        data: JSON.stringify(toSave, null, 2),
      });
      setSelectedName(name);
      localStorage.setItem(LAST_PROJECT_KEY, name);
      if (isCreating) {
        trackProjectCreated(name);
        setIsCreating(false);
        await loadProjects();
      } else {
        trackProjectUpdated(name, {
          agent_count: toSave.agents.length,
          skill_count: toSave.skills.length,
          mcp_count: (toSave.mcp_servers ?? []).length,
        });
      }
      setError(null);

      setSyncStatus(toSave.directory && toSave.agents.length > 0
        ? "Saved & synced"
        : "Saved");
      if (toSave.directory && toSave.agents.length > 0) {
        setDriftReport({ drifted: false, agents: [] });
        setDriftByProject((prev) => ({ ...prev, [name]: false }));
      }

      // Write any pending unified instruction content from a template apply
      const pending = pendingUnifiedInstruction.current;
      if (pending !== null && toSave.directory && toSave.agents.length > 0) {
        pendingUnifiedInstruction.current = null;
        // If the template had rules, persist them into file_rules before writing
        if (pending.rules.length > 0) {
          const latestRaw: string = await invoke("read_project", { name });
          const latestProj = JSON.parse(latestRaw);
          const withRules = {
            ...latestProj,
            file_rules: { ...(latestProj.file_rules || {}), _unified: pending.rules },
          };
          await invoke("save_project", { name, data: JSON.stringify(withRules, null, 2) });
        }
        await invoke("save_project_file", {
          name,
          filename: "_unified",
          content: pending.content,
        });
      }

      // Reload UI state from disk (picks up autodetected changes)
      await reloadProject(name);

      setTimeout(() => setSyncStatus(null), 4000);
    } catch (err: any) {
      setSyncStatus(null);
      setError(`Failed to save project: ${err}`);
    }
  };

  const handleRemove = async (name: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    const confirmed = await ask(`Remove project "${name}" from Automatic?\n\n(This only removes the project from this app. Your actual project files will NOT be deleted.)`, { title: "Remove Project", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_project", { name });
      trackProjectDeleted(name);
      if (selectedName === name) {
        setSelectedName(null);
        localStorage.removeItem(LAST_PROJECT_KEY);
        setProject(null);
        setDirty(false);
      }
      await loadProjects();
      setError(null);
    } catch (err: any) {
      setError(`Failed to remove project: ${err}`);
    }
  };

  const startCreate = async () => {
    setSelectedName(null);
    localStorage.removeItem(LAST_PROJECT_KEY);
    // Pre-populate agents from the "Default Agents" setting
    let defaultAgents: string[] = [];
    try {
      const raw: any = await invoke("read_settings");
      defaultAgents = raw.default_agents ?? [];
    } catch {
      // Non-fatal — proceed with empty agents if settings can't be read
    }
    setProject({ ...emptyProject(""), agents: defaultAgents });
    setDirty(true);
    setIsCreating(true);
    setNewName("");
    setSelectedProjectTemplate(null);
    setShowProjectTemplatePicker(false);
    setWizardStep(1);
    setWizardDiscoveredAgents([]);
    setWizardDiscovering(false);
  };

  const startRename = () => {
    if (!selectedName || isCreating) return;
    setRenameName(selectedName);
    setIsRenaming(true);
  };

  const handleRename = async () => {
    const trimmed = renameName.trim();
    if (!selectedName || !trimmed || trimmed === selectedName) {
      setIsRenaming(false);
      return;
    }
    try {
      await invoke("rename_project", { oldName: selectedName, newName: trimmed });
      // Update localStorage order
      const stored = localStorage.getItem(PROJECT_ORDER_KEY);
      if (stored) {
        try {
          const order: string[] = JSON.parse(stored);
          const idx = order.indexOf(selectedName);
          if (idx !== -1) {
            order[idx] = trimmed;
            localStorage.setItem(PROJECT_ORDER_KEY, JSON.stringify(order));
          }
        } catch { /* ignore */ }
      }
      setSelectedName(trimmed);
      localStorage.setItem(LAST_PROJECT_KEY, trimmed);
      setIsRenaming(false);
      setError(null);
      await loadProjects();
      await selectProject(trimmed);
    } catch (err: any) {
      setError(`Failed to rename project: ${err}`);
      setIsRenaming(false);
    }
  };

  // ── List helpers ─────────────────────────────────────────────────────────

  type ListField = "skills" | "mcp_servers" | "providers" | "agents";

  const addItem = (key: ListField, item: string) => {
    if (!project || !item.trim()) return;
    if (project[key].includes(item.trim())) return;
    updateField(key, [...project[key], item.trim()]);
    const pName = isCreating ? newName.trim() : (selectedName ?? "");
    if (key === "agents") trackProjectAgentAdded(pName, item.trim());
    else if (key === "skills") trackProjectSkillAdded(pName, item.trim());
    else if (key === "mcp_servers") trackProjectMcpServerAdded(pName, item.trim());
  };

  const removeItem = (key: ListField, idx: number) => {
    if (!project) return;
    const removed = project[key][idx];
    updateField(key, project[key].filter((_, i) => i !== idx));
    const pName = isCreating ? newName.trim() : (selectedName ?? "");
    if (removed) {
      if (key === "agents") trackProjectAgentRemoved(pName, removed);
      else if (key === "skills") trackProjectSkillRemoved(pName, removed);
      else if (key === "mcp_servers") trackProjectMcpServerRemoved(pName, removed);
    }
  };

  /**
   * Remove an agent from the project, prompting for confirmation and cleaning
   * up the agent's config files and skill directories from the project directory.
   *
   * If the project has no directory (not yet synced), falls back to an in-memory
   * removal so the user can save later.
   */
  const handleRemoveAgent = async (idx: number) => {
    if (!project) return;
    const agentId = project.agents[idx];
    if (!agentId) return;

    const agentInfo = availableAgents.find((a) => a.id === agentId);
    const agentLabel = agentInfo?.label ?? agentId;

    // If no directory or project not yet persisted → in-memory removal only
    if (!project.directory || !selectedName || isCreating) {
      const message = `Remove ${agentLabel} from this project?\n\nNo config files will be deleted since no project directory is configured.`;
      const confirmed = await ask(message, { title: "Remove Agent", kind: "warning" });
      if (!confirmed) return;
      removeItem("agents", idx);
      return;
    }

    const name = selectedName; // narrowed: guaranteed non-null from here on

    // Fetch the list of files that would be cleaned up (read-only preview)
    let preview: string[] = [];
    try {
      const raw: string = await invoke("get_agent_cleanup_preview", { name, agentId });
      preview = JSON.parse(raw);
    } catch {
      // Non-fatal — proceed with a generic message if the preview fails
    }

    const fileList =
      preview.length > 0
        ? `\n\nThe following files and directories will be deleted:\n${preview.map((p) => `  • ${p}`).join("\n")}`
        : "\n\nNo config files were found on disk for this agent.";

    const confirmed = await ask(
      `Remove ${agentLabel} from this project?${fileList}`,
      { title: "Remove Agent", kind: "warning" }
    );
    if (!confirmed) return;

    try {
      await invoke("remove_agent_from_project", { name, agentId });
      trackProjectAgentRemoved(name, agentId);
      await reloadProject(name);
      setDirty(false);
    } catch (err: any) {
      setError(`Failed to remove agent: ${err}`);
    }
  };

  const handleSync = async () => {
    const name = isCreating ? newName.trim() : selectedName;
    if (!name || !project) return;

    // Save first if dirty — handleSave already includes sync
    if (dirty) {
      await handleSave();
      return;
    }

    // Clean state: just re-sync from what's on disk
    try {
      setSyncStatus("syncing");
      const result: string = await invoke("sync_project", { name });
      const files: string[] = JSON.parse(result);
      trackProjectSynced(name);
      setSyncStatus(`Synced ${files.length} config${files.length !== 1 ? "s" : ""}`);
      setDriftReport({ drifted: false, agents: [] });
      setDriftByProject((prev) => ({ ...prev, [name]: false }));
    } catch (err: any) {
      setSyncStatus(`Sync failed: ${err}`);
    }

    await reloadProject(name);
    setTimeout(() => setSyncStatus(null), 4000);
  };

  const handleOpenInEditor = async (editorId: string) => {
    if (!project?.directory) return;
    setOpenInDropdownOpen(false);
    try {
      if (editorId === "copy_path") {
        await navigator.clipboard.writeText(project.directory);
      } else {
        await invoke("open_in_editor", { editorId, path: project.directory });
      }
    } catch (err: any) {
      setError(`Failed to open in editor: ${err}`);
    }
  };

  return (
    <>
    <div className="flex h-full w-full bg-bg-base">
      {/* Left sidebar - project list */}
      <div
        className="flex-shrink-0 flex flex-col border-r border-border-strong/40 bg-bg-input/50 relative"
        style={{ width: sidebarWidth }}
      >
        <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center bg-bg-base/30">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Projects
          </span>
          <button
            onClick={startCreate}
            className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
            title="Create New Project"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {projects.length === 0 && !isCreating ? (
            <div className="px-4 py-3 text-[13px] text-text-muted text-center">
              No projects yet.
            </div>
          ) : (
            <ul className="space-y-0.5 px-2" ref={listRef}>
              {isCreating && (
                <li className="flex items-center gap-2.5 px-2 py-1.5 rounded-md text-[13px] bg-bg-sidebar text-text-base">
                  <FolderOpen size={14} className="text-text-muted" />
                  <span className="italic">New Project...</span>
                </li>
              )}
              {projects.map((name, idx) => (
                <li
                  key={name}
                  className="relative"
                >
                  {/* Drop indicator line — above this item */}
                  {dragIdx !== null && dropIdx === idx && dropIdx !== dragIdx && dropIdx !== dragIdx + 1 && (
                    <div className="absolute -top-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                  )}
                  <div className={`group flex items-center relative ${dragIdx === idx ? "opacity-30" : ""}`}>
                    <div
                      className="absolute left-0 top-0 bottom-0 flex items-center pl-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing transition-opacity touch-none select-none z-10"
                      onPointerDown={(e) => handleGripDown(idx, e)}
                    >
                      <GripVertical size={10} className="text-text-muted" />
                    </div>
                    <button
                      onClick={() => { if (dragIdx === null) selectProject(name); }}
                      className={`w-full flex items-center gap-2.5 pl-4 pr-2 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
                        selectedName === name && !isCreating
                          ? "bg-bg-sidebar text-text-base"
                          : "text-text-muted hover:bg-bg-sidebar/50 hover:text-text-base"
                      }`}
                    >
                      <FolderOpen
                        size={14}
                        className={
                          driftByProject[name] === true
                            ? "text-warning"
                            : selectedName === name && !isCreating
                            ? "text-text-base"
                            : "text-text-muted"
                        }
                      />
                      <span className="flex-1 text-left truncate">{name}</span>
                    </button>
                    <button
                      onClick={(e) => handleRemove(name, e)}
                      className="absolute right-2 p-1 text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 hover:bg-surface rounded transition-all"
                      title="Remove Project from Automatic"
                    >
                      <X size={12} />
                    </button>
                  </div>
                  {/* Drop indicator line — after last item */}
                  {dragIdx !== null && dropIdx === projects.length && idx === projects.length - 1 && dropIdx !== dragIdx && (
                    <div className="absolute -bottom-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
        {/* Resize handle */}
        <div
          className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-brand/40 active:bg-brand/60 transition-colors z-10"
          onMouseDown={onSidebarMouseDown}
        />
      </div>

      {/* Right area - project detail */}
      <div className="flex-1 flex flex-col min-w-0 bg-bg-base">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between">
            {error}
            <button onClick={() => setError(null)}>
              <X size={14} />
            </button>
          </div>
        )}

        {project ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="h-11 px-6 border-b border-border-strong/40 flex justify-between items-center">
              <div className="flex items-center gap-3">
                <FolderOpen size={14} className="text-text-muted" />
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="project-name (no spaces/slashes)"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-64"
                  />
                ) : isRenaming ? (
                  <input
                    type="text"
                    value={renameName}
                    onChange={(e) => setRenameName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleRename();
                      if (e.key === "Escape") setIsRenaming(false);
                    }}
                    onBlur={handleRename}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-64"
                  />
                ) : (
                  <h3
                    className="text-[14px] font-medium text-text-base cursor-text"
                    onDoubleClick={startRename}
                    title="Double-click to rename"
                  >
                    {selectedName}
                  </h3>
                )}
              </div>

              <div className="flex items-center gap-2">
                {syncStatus && (
                  <span className={`text-[12px] ${syncStatus.startsWith("Sync failed") ? "text-danger" : syncStatus === "syncing" ? "text-text-muted" : "text-success"}`}>
                    {syncStatus === "syncing" ? "Syncing..." : syncStatus}
                  </span>
                )}
                {/* Open in editor dropdown — only shown when a directory is set */}
                {!isCreating && project.directory && (
                  <div className="relative mr-1" ref={openInDropdownRef}>
                    <button
                      onClick={() => setOpenInDropdownOpen((v) => !v)}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-bg-input hover:bg-surface-hover text-text-base rounded text-[12px] font-medium border border-border-strong transition-colors shadow-sm"
                      title="Open project in an editor"
                    >
                      <FolderOpen size={12} /> Open in
                      <ChevronDown size={11} className={`transition-transform ${openInDropdownOpen ? "rotate-180" : ""}`} />
                    </button>
                    {openInDropdownOpen && (
                      <div className="absolute right-0 top-full mt-1 w-44 bg-bg-input border border-border-strong/40 rounded-lg shadow-xl z-50 py-1 overflow-hidden">
                        {installedEditors.filter((e) => e.installed).map((editor) => (
                          <button
                            key={editor.id}
                            onClick={() => handleOpenInEditor(editor.id)}
                            className="w-full flex items-center gap-2.5 px-3 py-2 text-[13px] text-text-base hover:bg-bg-sidebar transition-colors text-left"
                          >
                            <EditorIcon id={editor.id} iconPath={editorIconPaths[editor.id]} />
                            {editor.label}
                          </button>
                        ))}
                        <div className="border-t border-border-strong/40 my-1" />
                        <button
                          onClick={() => handleOpenInEditor("copy_path")}
                          className="w-full flex items-center gap-2.5 px-3 py-2 text-[13px] text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors text-left"
                        >
                          <Copy size={13} />
                          Copy path
                        </button>
                      </div>
                    )}
                  </div>
                )}
                {!isCreating && selectedName && (
                  <button
                    onClick={() => handleRemove(selectedName)}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-bg-sidebar hover:bg-danger/10 text-text-muted hover:text-danger rounded text-[12px] font-medium border border-border-strong/40-hover hover:border-danger/30 transition-colors mr-1"
                    title="Remove project from Automatic"
                  >
                    <Trash2 size={12} /> Remove
                  </button>
                )}
                {/* Sync / in-sync indicator — shown when project has directory + agents configured */}
                {!dirty && project.directory && project.agents.length > 0 && (
                  driftReport?.drifted ? (
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-warning/10 hover:bg-warning/20 text-warning rounded text-[12px] font-medium border border-warning/40 hover:border-warning/60 transition-colors"
                      title="Configuration has drifted — click to sync"
                    >
                      <RefreshCw size={12} /> Sync Configs
                    </button>
                  ) : driftReport && !driftReport.drifted ? (
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-success rounded text-[12px] font-medium border border-success/20 hover:border-success/40 transition-colors"
                      title="Configuration is up to date — click to force sync"
                    >
                      <Check size={12} /> In Sync
                    </button>
                  ) : (
                    /* driftReport === null: not yet checked */
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-muted rounded text-[12px] font-medium border border-border-strong/40-hover transition-colors"
                      title="Sync agent configurations"
                    >
                      <RefreshCw size={12} /> Sync Configs
                    </button>
                  )
                )}
                {dirty && (
                  <button
                    onClick={handleSave}
                    disabled={isCreating && !newName.trim()}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                  >
                    <Check size={12} /> Save
                  </button>
                )}
              </div>
            </div>

            {/* ── Drift warning banner ─────────────────────────────── */}
            {driftReport?.drifted && !dirty && !isCreating && project.directory && project.agents.length > 0 && (
              <div className="border-b border-warning/25 bg-warning/10">
                <div className="flex items-center justify-between px-6 py-2 text-warning">
                  <div className="flex items-center gap-2 text-[12px]">
                    <AlertCircle size={13} />
                    <span>Configuration has drifted — agent config files no longer match Automatic settings.</span>
                  </div>
                  <button
                    onClick={handleSync}
                    className="text-[12px] font-medium text-warning hover:text-warning-hover underline decoration-warning/40 hover:decoration-warning-hover transition-colors ml-4 flex-shrink-0"
                  >
                    Sync now
                  </button>
                </div>
                {/* Detail: which agents/files have drifted — click any file to view the diff */}
                <div className="px-6 pb-3 space-y-1.5">
                  {driftReport.agents.map((agentDrift) => (
                    <div key={agentDrift.agent_id}>
                      <div className="text-[11px] font-semibold text-warning/80 mb-0.5">{agentDrift.agent_label}</div>
                      <div className="flex flex-wrap gap-x-2 gap-y-1">
                        {agentDrift.files.map((f, i) => (
                          <button
                            key={i}
                            onClick={() => setDriftDiffFile({ file: f, agentLabel: agentDrift.agent_label })}
                            className="flex items-center gap-1 text-[11px] font-mono text-warning/70 hover:text-warning bg-warning/5 hover:bg-warning/15 border border-warning/20 hover:border-warning/40 rounded px-1.5 py-0.5 transition-colors"
                            title="View diff"
                          >
                            {f.path}
                            <span className="text-warning/50 font-sans ml-0.5">({f.reason})</span>
                          </button>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* ── New project wizard (3 steps) ─────────────────────── */}
            {isCreating && (
              <div className="flex-1 flex flex-col items-center justify-center p-8">
                <div className="w-full max-w-md">

                  {/* Step indicator */}
                  <div className="flex items-center justify-center gap-2 mb-8">
                    {([1, 2, 3] as const).map((s) => (
                      <div key={s} className="flex items-center gap-2">
                        <div className={`w-6 h-6 rounded-full flex items-center justify-center text-[11px] font-semibold transition-colors ${
                          wizardStep === s
                            ? "bg-brand text-text-base"
                            : wizardStep > s
                            ? "bg-brand/30 text-brand"
                            : "bg-bg-sidebar text-text-muted"
                        }`}>
                          {wizardStep > s ? <Check size={11} /> : s}
                        </div>
                        {s < 3 && (
                          <div className={`w-8 h-px ${wizardStep > s ? "bg-brand/50" : "bg-surface"}`} />
                        )}
                      </div>
                    ))}
                  </div>

                  {/* ── Step 1: Directory ──────────────────────────────── */}
                  {wizardStep === 1 && (
                    <>
                      <div className="mb-8 text-center">
                        <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-brand/10 border border-brand/30 flex items-center justify-center">
                          <FolderOpen size={24} className="text-brand" strokeWidth={1.5} />
                        </div>
                        <h2 className="text-[16px] font-semibold text-text-base mb-1">Where is this project?</h2>
                        <p className="text-[13px] text-text-muted leading-relaxed">
                          Choose an existing project directory — Automatic will scan it and detect your agents automatically.
                        </p>
                      </div>

                      <div className="space-y-3">
                        <div className="flex gap-2">
                          <input
                            type="text"
                            value={project.directory}
                            onChange={(e) => updateField("directory", e.target.value)}
                            placeholder="/path/to/your/project"
                            className="flex-1 bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors"
                          />
                          <button
                            onClick={async () => {
                              const selected = await open({
                                directory: true,
                                multiple: false,
                                title: "Select project directory",
                              });
                              if (!selected) return;
                              const dir = selected as string;
                              const folderName = dir.split("/").filter(Boolean).pop() ?? "";
                              const name = newName.trim() || folderName;
                              setNewName(name);
                              updateField("directory", dir);
                            }}
                            className="px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors whitespace-nowrap"
                          >
                            Browse
                          </button>
                        </div>

                        {project.directory && (
                          <button
                            disabled={wizardDiscovering}
                            onClick={async () => {
                              const dir = project.directory.trim();
                              if (!dir) return;
                              const folderName = dir.split("/").filter(Boolean).pop() ?? "";
                              const name = newName.trim() || folderName;
                              setNewName(name);
                              setWizardDiscovering(true);
                              setError(null);
                              try {
                                // Save minimal stub so autodetect can read it back
                                const stub = { ...emptyProject(name), directory: dir, name };
                                if (userId && !stub.created_by) stub.created_by = userId;
                                await invoke("save_project", { name, data: JSON.stringify(stub, null, 2) });
                                // Run read-only autodetection
                                const raw: string = await invoke("autodetect_project_dependencies", { name });
                                const detected = JSON.parse(raw) as Project;
                                // Merge default agents (already set on project from startCreate)
                                // with autodetected agents so neither source is lost.
                                const mergedAgents = [
                                  ...new Set([...(project?.agents ?? []), ...detected.agents]),
                                ];
                                // Pre-fill wizard project with discovered agents/skills/servers
                                setProject({
                                  ...stub,
                                  agents: mergedAgents,
                                  skills: detected.skills,
                                  local_skills: detected.local_skills,
                                  mcp_servers: detected.mcp_servers,
                                });
                                setWizardDiscoveredAgents(detected.agents);
                                setWizardStep(2);
                              } catch (err: any) {
                                setError(`Autodetect failed: ${err}`);
                              } finally {
                                setWizardDiscovering(false);
                              }
                            }}
                            className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-white text-[13px] font-medium rounded shadow-sm transition-colors"
                          >
                            {wizardDiscovering ? (
                              <><RefreshCw size={13} className="animate-spin" /> Scanning…</>
                            ) : (
                              <><ArrowRight size={13} /> Continue</>
                            )}
                          </button>
                        )}
                      </div>
                    </>
                  )}

                  {/* ── Step 2: Agents ────────────────────────────────── */}
                  {wizardStep === 2 && (
                    <>
                      <div className="mb-6 text-center">
                        <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-brand/10 border border-brand/30 flex items-center justify-center">
                          <Bot size={24} className="text-brand" strokeWidth={1.5} />
                        </div>
                        <h2 className="text-[16px] font-semibold text-text-base mb-1">Which agents are you using?</h2>
                        <p className="text-[13px] text-text-muted leading-relaxed">
                          {wizardDiscoveredAgents.length > 0
                            ? `We detected ${wizardDiscoveredAgents.length} agent${wizardDiscoveredAgents.length !== 1 ? "s" : ""} in this directory. Add or remove as needed.`
                            : "No agents were detected. Add the ones you use."}
                        </p>
                      </div>

                      {/* Agent toggle list */}
                      <div className="space-y-2 mb-4 max-h-56 overflow-y-auto custom-scrollbar">
                        {project.agents.map((id, idx) => {
                          const info = availableAgents.find((a) => a.id === id);
                          const isDiscovered = wizardDiscoveredAgents.includes(id);
                          return (
                            <div
                              key={id}
                              className="flex items-center gap-3 px-3 py-2.5 bg-bg-input border border-border-strong/40 rounded-lg"
                            >
                              <AgentIcon agentId={id} size={18} />
                              <div className="flex-1 min-w-0">
                                <div className="text-[13px] font-medium text-text-base">{info?.label ?? id}</div>
                                {isDiscovered && (
                                  <div className="text-[10px] text-brand mt-0.5">Detected in directory</div>
                                )}
                              </div>
                              <button
                                onClick={() => removeItem("agents", idx)}
                                className="p-1 text-text-muted hover:text-danger hover:bg-surface rounded transition-colors"
                                title="Remove"
                              >
                                <X size={12} />
                              </button>
                            </div>
                          );
                        })}
                        {project.agents.length === 0 && (
                          <p className="text-[12px] text-text-muted italic px-1">No agents selected.</p>
                        )}
                      </div>

                      {/* Add more agents inline */}
                      {(() => {
                        const unaddedAgents = availableAgents.filter((a) => !project.agents.includes(a.id));
                        return unaddedAgents.length > 0 ? (
                          <div className="mt-1">
                            <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">Add agent</div>
                            <div className="space-y-1 max-h-36 overflow-y-auto custom-scrollbar">
                              {unaddedAgents.map((a) => (
                                <button
                                  key={a.id}
                                  onClick={() => addItem("agents", a.id)}
                                  className="w-full flex items-center gap-2.5 px-3 py-2 bg-bg-input hover:bg-bg-sidebar border border-border-strong/40 hover:border-border-strong rounded-md text-left transition-colors"
                                >
                                  <AgentIcon agentId={a.id} size={14} />
                                  <div className="flex-1 min-w-0">
                                    <span className="text-[13px] text-text-base font-medium">{a.label}</span>
                                    {a.description && (
                                      <span className="text-[11px] text-text-muted ml-2">{a.description}</span>
                                    )}
                                  </div>
                                  <Plus size={11} className="text-brand flex-shrink-0" />
                                </button>
                              ))}
                            </div>
                          </div>
                        ) : null;
                      })()}

                      <div className="flex gap-2 mt-6">
                        <button
                          onClick={() => setWizardStep(1)}
                          className="flex-1 px-4 py-2.5 bg-bg-sidebar hover:bg-surface text-text-muted hover:text-text-base text-[13px] font-medium rounded transition-colors"
                        >
                          Back
                        </button>
                        <button
                          onClick={() => setWizardStep(3)}
                          className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
                        >
                          <ArrowRight size={13} /> Continue
                        </button>
                      </div>
                    </>
                  )}

                  {/* ── Step 3: Template ──────────────────────────────── */}
                  {wizardStep === 3 && (
                    <>
                      <div className="mb-6 text-center">
                        <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-brand/10 border border-brand/30 flex items-center justify-center">
                          <LayoutTemplate size={24} className="text-brand" strokeWidth={1.5} />
                        </div>
                        <h2 className="text-[16px] font-semibold text-text-base mb-1">Apply a template</h2>
                        <p className="text-[13px] text-text-muted leading-relaxed">
                          Optionally start from a project template to pre-configure skills, MCP servers, and instructions.
                        </p>
                      </div>

                      {availableProjectTemplates.length > 0 ? (
                        <div className="space-y-1 max-h-56 overflow-y-auto custom-scrollbar mb-5">
                          {availableProjectTemplates.map((tmpl) => {
                            const isSelected = selectedProjectTemplate === tmpl.name;
                            return (
                              <button
                                key={tmpl.name}
                                onClick={() => applyProjectTemplate(tmpl)}
                                className={`w-full text-left px-3 py-2.5 rounded-md transition-colors flex items-start gap-2 border ${
                                  isSelected
                                    ? "bg-brand/15 border-brand/40"
                                    : "bg-bg-input border-border-strong/40 hover:border-border-strong hover:bg-bg-sidebar"
                                }`}
                              >
                                <div className="flex-1 min-w-0">
                                  <div className="text-[13px] font-medium text-text-base">{tmpl.name}</div>
                                  {tmpl.description && (
                                    <div className="text-[11px] text-text-muted mt-0.5 truncate">{tmpl.description}</div>
                                  )}
                                  <div className="flex items-center gap-3 mt-1">
                                    {tmpl.agents.length > 0 && (
                                      <span className="text-[10px] text-text-muted flex items-center gap-1">
                                        <Bot size={10} /> {tmpl.agents.length}
                                      </span>
                                    )}
                                    {tmpl.skills.length > 0 && (
                                      <span className="text-[10px] text-text-muted flex items-center gap-1">
                                        <Code size={10} /> {tmpl.skills.length}
                                      </span>
                                    )}
                                    {tmpl.mcp_servers.length > 0 && (
                                      <span className="text-[10px] text-text-muted flex items-center gap-1">
                                        <Server size={10} /> {tmpl.mcp_servers.length}
                                      </span>
                                    )}
                                  </div>
                                </div>
                                {isSelected && (
                                  <Check size={13} className="text-brand flex-shrink-0 mt-0.5" />
                                )}
                              </button>
                            );
                          })}
                        </div>
                      ) : (
                        <div className="mb-5 px-3 py-4 bg-bg-input border border-border-strong/40 rounded-md text-center">
                          <p className="text-[12px] text-text-muted italic">No project templates configured.</p>
                        </div>
                      )}

                      {selectedProjectTemplate && (
                        <button
                          onClick={() => {
                            setProject((p) => p ? { ...p, agents: p.agents } : p);
                            setSelectedProjectTemplate(null);
                            setShowProjectTemplatePicker(false);
                          }}
                          className="flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base mb-4 transition-colors"
                        >
                          <X size={11} /> Clear template
                        </button>
                      )}

                      <div className="flex gap-2">
                        <button
                          onClick={() => setWizardStep(2)}
                          className="flex-1 px-4 py-2.5 bg-bg-sidebar hover:bg-surface text-text-muted hover:text-text-base text-[13px] font-medium rounded transition-colors"
                        >
                          Back
                        </button>
                        <button
                          onClick={handleSave}
                          className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
                        >
                          <Check size={13} /> Create Project
                        </button>
                      </div>
                    </>
                  )}

                </div>
              </div>
            )}

            {/* Tab bar + content (hidden while in new-project setup) */}
            {!isCreating && <>
            <div className="flex flex-wrap items-center gap-0 px-6 border-b border-border-strong/40 bg-bg-base">
              {([
                 { id: "summary" as ProjectTab, label: "Summary" },
                 { id: "agents" as ProjectTab, label: "Agents" },
                { id: "skills" as ProjectTab, label: "Skills" },
                { id: "mcp_servers" as ProjectTab, label: "MCP Servers" },
                { id: "project_file" as ProjectTab, label: "Project Instructions" },
                { id: "memory" as ProjectTab, label: "Memory" },
              ]).map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setProjectTab(tab.id)}
                  className={`px-3 py-2 text-[13px] font-medium transition-colors relative ${
                    projectTab === tab.id
                      ? "text-text-base"
                      : "text-text-muted hover:text-text-base"
                  }`}
                >
                  {tab.label}
                  {projectTab === tab.id && (
                    <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-brand rounded-t" />
                  )}
                </button>
              ))}
            </div>

            {/* Tab content */}

            {/* ── Project File tab (full-bleed layout) ──────────── */}
            {projectTab === "project_file" && (
              <>
                {project.directory && project.agents.length > 0 ? (
                  <div className="flex-1 flex flex-col min-h-0">
                    {/* Mode toggle bar */}
                    <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border-strong/40 bg-bg-input/30 flex-shrink-0">
                      <span className="text-[11px] text-text-muted">Mode:</span>
                      <div className="flex rounded overflow-hidden border border-border-strong/40">
                        <button
                          onClick={async () => {
                            if (project.instruction_mode !== "unified" && selectedName) {
                              const updated = { ...project, instruction_mode: "unified", updated_at: new Date().toISOString() };
                              setProject(updated);
                              setDirty(false);
                              await invoke("save_project", { name: selectedName, data: JSON.stringify(updated, null, 2) });
                              await loadProjectFiles(selectedName);
                            }
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                            (project.instruction_mode || "per-agent") === "unified"
                              ? "bg-brand text-text-base"
                              : "bg-bg-sidebar text-text-muted hover:text-text-base"
                          }`}
                        >
                          <Files size={11} />
                          Unified
                        </button>
                        <button
                          onClick={async () => {
                            if (project.instruction_mode !== "per-agent" && selectedName) {
                              const updated = { ...project, instruction_mode: "per-agent", updated_at: new Date().toISOString() };
                              setProject(updated);
                              setDirty(false);
                              await invoke("save_project", { name: selectedName, data: JSON.stringify(updated, null, 2) });
                              await loadProjectFiles(selectedName);
                            }
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                            (project.instruction_mode || "per-agent") === "per-agent"
                              ? "bg-brand text-text-base"
                              : "bg-bg-sidebar text-text-muted hover:text-text-base"
                          }`}
                        >
                          <SplitSquareHorizontal size={11} />
                          Per Agent
                        </button>
                      </div>
                      {(project.instruction_mode || "per-agent") === "unified" && projectFiles.length > 0 && projectFiles[0].target_files && (
                        <span className="text-[10px] text-text-muted">
                          Writes to: {projectFiles[0].target_files.join(", ")}
                        </span>
                      )}
                    </div>

                    <div className="flex-1 flex min-h-0">
                    {/* File sidebar — hidden in unified mode */}
                    {(project.instruction_mode || "per-agent") === "per-agent" && projectFiles.length > 0 && (
                      <div className="w-52 flex-shrink-0 border-r border-border-strong/40 bg-bg-input/50 flex flex-col">
                        <div className="h-9 px-3 border-b border-border-strong/40 flex items-center justify-between">
                          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Files</span>
                          <button
                            onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                            className="text-text-muted hover:text-text-base p-0.5 hover:bg-bg-sidebar rounded transition-colors"
                            title="Start from template"
                          >
                            <LayoutTemplate size={12} />
                          </button>
                        </div>
                        <div className="flex-1 overflow-y-auto py-1.5 custom-scrollbar">
                          <ul className="space-y-0.5 px-1.5">
                            {projectFiles.map((f) => (
                              <li key={f.filename}>
                                <button
                                  onClick={async () => {
                                    if (projectFileDirty && !(await ask("Discard unsaved changes?", { title: "Unsaved Changes", kind: "warning" }))) return;
                                    setActiveProjectFile(f.filename);
                                    if (selectedName) await loadProjectFileContent(selectedName, f.filename);
                                  }}
                                  className={`w-full text-left px-2.5 py-1.5 rounded-md text-[13px] font-medium transition-colors flex items-center gap-2 ${
                                    activeProjectFile === f.filename
                                      ? "bg-bg-sidebar text-text-base"
                                      : "text-text-muted hover:bg-bg-sidebar/50 hover:text-text-base"
                                  }`}
                                >
                                  <FileText size={13} className={activeProjectFile === f.filename ? "text-text-base" : f.exists ? "text-text-muted" : "text-text-muted"} />
                                  <div className="min-w-0">
                                    <div className={`truncate ${!f.exists ? "opacity-50" : ""}`}>{f.filename}</div>
                                    <div className="text-[10px] text-text-muted truncate">{f.agents.join(", ")}</div>
                                  </div>
                                </button>
                              </li>
                            ))}
                          </ul>
                        </div>
                        {/* Template picker (dropdown in sidebar) */}
                        {showTemplatePicker && availableTemplates.length > 0 && (
                          <div className="border-t border-border-strong/40 p-2">
                            <p className="text-[10px] text-text-muted mb-1.5">Apply template:</p>
                            <div className="space-y-0.5">
                              {availableTemplates.map((t) => (
                                <button
                                  key={t}
                                  onClick={() => handleApplyTemplate(t)}
                                  className="w-full text-left px-2 py-1 text-[12px] bg-bg-sidebar hover:bg-brand text-text-base rounded transition-colors flex items-center gap-1.5"
                                >
                                  <LayoutTemplate size={10} />
                                  {t}
                                </button>
                              ))}
                            </div>
                          </div>
                        )}
                      </div>
                    )}

                    {/* Editor area (fills remaining space) */}
                    {projectFiles.length > 0 && activeProjectFile ? (() => {
                      const activeFile = projectFiles.find(f => f.filename === activeProjectFile);
                      const fileExists = activeFile?.exists ?? false;

                      if (!fileExists && !projectFileEditing) {
                        // File doesn't exist yet — show create prompt
                        return (
                          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
                            <div className="w-12 h-12 mx-auto mb-4 rounded-full border border-dashed border-border-strong flex items-center justify-center text-text-muted">
                              <FileText size={20} strokeWidth={1.5} />
                            </div>
                            <h3 className="text-[14px] font-medium text-text-base mb-1">
                              {activeProjectFile}
                            </h3>
                            <p className="text-[13px] text-text-muted mb-5 max-w-xs">
                              This file doesn't exist yet. Create it to provide project instructions for {activeFile?.agents.join(" & ")}.
                            </p>
                            <div className="flex items-center gap-2">
                              <button
                                onClick={() => {
                                  setProjectFileContent("");
                                  setProjectFileEditing(true);
                                  setProjectFileDirty(true);
                                  // Auto-enable Automatic rule when creating a new file
                                  if (project && activeProjectFile) {
                                    const hasAutomatic = availableRules.some(r => r.id === "automatic-service");
                                    if (hasAutomatic && !project.file_rules?.[activeProjectFile]) {
                                      setProject({ ...project, file_rules: { ...(project.file_rules || {}), [activeProjectFile]: ["automatic-service"] } });
                                    }
                                  }
                                }}
                                className="px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5"
                              >
                                <Plus size={12} /> Create File
                              </button>
                              {availableTemplates.length > 0 && (
                                <button
                                  onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                                  className="px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-border-strong/40-hover transition-colors flex items-center gap-1.5"
                                >
                                  <LayoutTemplate size={12} /> From Template
                                </button>
                              )}
                            </div>
                            {showTemplatePicker && availableTemplates.length > 0 && (
                              <div className="mt-3 p-2 bg-bg-input rounded-md border border-border-strong/40">
                                <div className="flex flex-wrap gap-1.5">
                                  {availableTemplates.map((t) => (
                                    <button
                                      key={t}
                                      onClick={() => handleApplyTemplate(t)}
                                      className="px-2 py-1 text-[12px] bg-bg-sidebar hover:bg-brand text-text-base rounded transition-colors flex items-center gap-1.5"
                                    >
                                      <LayoutTemplate size={10} />
                                      {t}
                                    </button>
                                  ))}
                                </div>
                              </div>
                            )}
                          </div>
                        );
                      }

                      // File exists or we're editing a new file
                      const currentFileRules = (project.file_rules || {})[activeProjectFile] || [];

                      const handleToggleRule = (ruleName: string) => {
                        if (!project || !activeProjectFile) return;
                        const existing = (project.file_rules || {})[activeProjectFile] || [];
                        const updated = existing.includes(ruleName)
                          ? existing.filter(r => r !== ruleName)
                          : [...existing, ruleName];
                        const newFileRules = { ...(project.file_rules || {}), [activeProjectFile]: updated };
                        if (updated.length === 0) delete newFileRules[activeProjectFile];
                        setProject({ ...project, file_rules: newFileRules });
                        setDirty(true);
                      };

                      return (
                        <div className="flex-1 flex flex-col min-w-0">
                          {/* Editor toolbar */}
                          <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
                            <span className="text-[11px] text-text-muted">
                              {activeProjectFile === "_unified"
                                ? <>{projectFileEditing ? "Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                                : <>{activeProjectFile}{!fileExists ? " (new)" : ""}{projectFileEditing ? " — Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                              }
                            </span>
                            <div className="flex items-center gap-1.5">
                              {!projectFileEditing ? (
                                <button
                                  onClick={() => {
                                    setProjectFileEditing(true);
                                    // Auto-enable Automatic rule if rules have never been configured for this file
                                    if (project && activeProjectFile) {
                                      const hasAutomatic = availableRules.some(r => r.id === "automatic-service");
                                      if (hasAutomatic && !project.file_rules?.[activeProjectFile]) {
                                        setProject({ ...project, file_rules: { ...(project.file_rules || {}), [activeProjectFile]: ["automatic-service"] } });
                                      }
                                    }
                                  }}
                                  className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                >
                                  <Edit2 size={10} /> Edit
                                </button>
                              ) : (
                                <>
                                  <button
                                    onClick={() => {
                                      setProjectFileEditing(false);
                                      if (projectFileDirty && selectedName && activeProjectFile) {
                                        if (fileExists) {
                                          loadProjectFileContent(selectedName, activeProjectFile);
                                        } else {
                                          setProjectFileContent("");
                                          setProjectFileDirty(false);
                                        }
                                      }
                                    }}
                                    className="px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                  >
                                    Cancel
                                  </button>
                                  <button
                                    onClick={handleSaveProjectFile}
                                    disabled={!projectFileDirty || projectFileSaving}
                                    className="flex items-center gap-1 px-2 py-0.5 text-[11px] bg-brand hover:bg-brand-hover text-white rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                                  >
                                    <Check size={10} /> {projectFileSaving ? "Saving..." : "Save"}
                                  </button>
                                </>
                              )}
                            </div>
                          </div>

                          {/* Content area — fills remaining height above rules panel */}
                          {projectFileEditing ? (
                            <textarea
                              value={projectFileContent}
                              onChange={(e) => {
                                setProjectFileContent(e.target.value);
                                setProjectFileDirty(true);
                              }}
                              className="flex-1 w-full p-4 resize-none outline-none font-mono text-[12px] bg-bg-base text-text-base leading-relaxed custom-scrollbar placeholder-text-muted/30 min-h-0"
                              placeholder="Write your project instructions here..."
                              spellCheck={false}
                            />
                          ) : (
                            <div className="flex-1 overflow-y-auto custom-scrollbar bg-bg-base min-h-0">
                              {projectFileContent
                                ? <MarkdownPreview content={projectFileContent} />
                                : <span className="block p-4 text-[13px] text-text-muted italic">Empty file.</span>
                              }
                            </div>
                          )}

                          {/* Rules panel */}
                          <div className="border-t border-border-strong/40 bg-bg-input flex-shrink-0">
                            <div className="px-4 py-2 flex items-center gap-2">
                              <ScrollText size={12} className="text-accent-hover" />
                              <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Rules</span>
                              {currentFileRules.length > 0 && (
                                <span className="text-[10px] text-accent-hover bg-accent-hover/10 px-1.5 py-0.5 rounded">{currentFileRules.length}</span>
                              )}
                            </div>
                            {availableRules.length > 0 ? (
                              <div className="px-4 pb-3 flex flex-wrap gap-1.5">
                                {availableRules.map(rule => {
                                  const isSelected = currentFileRules.includes(rule.id);
                                  return (
                                    <button
                                      key={rule.id}
                                      onClick={() => handleToggleRule(rule.id)}
                                      className={`px-2.5 py-1 text-[12px] rounded border transition-colors flex items-center gap-1.5 ${
                                        isSelected
                                          ? "bg-accent-hover/8 border-accent-hover/25 text-accent-hover/75"
                                          : "bg-bg-sidebar border-border-strong/40 text-text-muted hover:text-text-base hover:border-border-strong"
                                      }`}
                                    >
                                      <ScrollText size={10} />
                                      {rule.name}
                                      {isSelected && <Check size={10} />}
                                    </button>
                                  );
                                })}
                              </div>
                            ) : (
                              <div className="px-4 pb-3">
                                <span className="text-[11px] text-text-muted italic">No rules created yet. Create rules in the Rules section to attach them here.</span>
                              </div>
                            )}
                          </div>
                        </div>
                      );
                    })() : (
                      <div className="flex-1 flex items-center justify-center">
                        <p className="text-[13px] text-text-muted italic">No project files configured. Add agent tools on the Agents tab first.</p>
                      </div>
                    )}
                  </div>
                  </div>
                ) : (
                  <div className="flex-1 flex items-center justify-center">
                    <p className="text-[13px] text-text-muted italic">
                      Set a project directory and add agent tools on the Details and Agents tabs to manage project files.
                    </p>
                  </div>
                )}
              </>
            )}

            {/* Other tabs (padded container) */}
            {projectTab !== "project_file" && (
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="space-y-8">

                {/* ── Summary tab ──────────────────────────────────────── */}
                {projectTab === "summary" && (
                  <>
                    {/* Quick Stats */}
                    <div className="grid grid-cols-2 xl:grid-cols-4 gap-4">
                      {/* Agents Card */}
                      <button
                        onClick={() => setProjectTab("agents")}
                        className="group bg-bg-input border border-border-strong/40 hover:border-brand/50 rounded-lg p-4 text-left transition-all hover:shadow-lg hover:shadow-brand/10"
                      >
                        <div className="flex items-start gap-3">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-start justify-between">
                              <div className="text-3xl font-semibold text-text-base leading-none mb-1">
                                {project.agents.length}
                              </div>
                              <ArrowRight size={14} className="text-text-muted opacity-0 group-hover:opacity-100 transition-opacity mt-0.5" />
                            </div>
                             <div className="text-[13px] text-text-muted mb-1">Agent Tools</div>
                             <div className="flex items-center gap-1.5 mt-1 flex-wrap">
                               {project.agents.length === 0
                                 ? <span className="text-[11px] text-text-muted">No agents configured</span>
                                 : <>
                                     {project.agents.slice(0, 4).map(a => (
                                       <AgentIcon key={a} agentId={a} size={16} className="text-text-muted" />
                                     ))}
                                     {project.agents.length > 4 && (
                                       <span className="text-[11px] text-text-muted">+{project.agents.length - 4}</span>
                                     )}
                                   </>
                               }
                             </div>
                          </div>
                          <div className="p-2 bg-brand/10 rounded-lg group-hover:bg-brand/20 transition-colors shrink-0">
                            <Bot size={18} className="text-brand" />
                          </div>
                        </div>
                      </button>

                      {/* Skills Card */}
                      <button
                        onClick={() => setProjectTab("skills")}
                        className="group bg-bg-input border border-border-strong/40 hover:border-icon-skill/50 rounded-lg p-4 text-left transition-all hover:shadow-lg hover:shadow-icon-skill/10"
                      >
                        <div className="flex items-start gap-3">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-start justify-between">
                              <div className="text-3xl font-semibold text-text-base leading-none mb-1">
                                {project.skills.length + project.local_skills.length}
                              </div>
                              <ArrowRight size={14} className="text-text-muted opacity-0 group-hover:opacity-100 transition-opacity mt-0.5" />
                            </div>
                            <div className="text-[13px] text-text-muted mb-1">Skills</div>
                            <div className="text-[11px] text-text-muted truncate">
                              {project.skills.length === 0 && project.local_skills.length === 0
                                ? "No skills attached"
                                : `${project.skills.length} global, ${project.local_skills.length} local`}
                            </div>
                          </div>
                          <div className="p-2 bg-icon-skill/10 rounded-lg group-hover:bg-icon-skill/20 transition-colors shrink-0">
                            <Code size={18} className="text-icon-skill" />
                          </div>
                        </div>
                      </button>

                      {/* MCP Servers Card */}
                      <button
                        onClick={() => setProjectTab("mcp_servers")}
                        className="group bg-bg-input border border-border-strong/40 hover:border-icon-mcp/50 rounded-lg p-4 text-left transition-all hover:shadow-lg hover:shadow-icon-mcp/10"
                      >
                        <div className="flex items-start gap-3">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-start justify-between">
                              <div className="text-3xl font-semibold text-text-base leading-none mb-1">
                                {project.mcp_servers.length}
                              </div>
                              <ArrowRight size={14} className="text-text-muted opacity-0 group-hover:opacity-100 transition-opacity mt-0.5" />
                            </div>
                            <div className="text-[13px] text-text-muted mb-1">MCP Servers</div>
                            <div className="text-[11px] text-text-muted truncate">
                              {project.mcp_servers.length === 0
                                ? "No servers configured"
                                : project.mcp_servers.slice(0, 2).join(", ") + (project.mcp_servers.length > 2 ? ` +${project.mcp_servers.length - 2}` : "")}
                            </div>
                          </div>
                          <div className="p-2 bg-icon-mcp/10 rounded-lg group-hover:bg-icon-mcp/20 transition-colors shrink-0">
                            <Server size={18} className="text-icon-mcp" />
                          </div>
                        </div>
                      </button>

                      {/* Memory Card */}
                      <button
                        onClick={() => setProjectTab("memory")}
                        className="group bg-bg-input border border-border-strong/40 hover:border-icon-rule/50 rounded-lg p-4 text-left transition-all hover:shadow-lg hover:shadow-icon-rule/10"
                      >
                        <div className="flex items-start gap-3">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-start justify-between">
                              <div className="text-3xl font-semibold text-text-base leading-none mb-1">
                                {Object.keys(memories).length}
                              </div>
                              <ArrowRight size={14} className="text-text-muted opacity-0 group-hover:opacity-100 transition-opacity mt-0.5" />
                            </div>
                            <div className="text-[13px] text-text-muted mb-1">Memory</div>
                            <div className="text-[11px] text-text-muted truncate">
                              {Object.keys(memories).length === 0
                                ? "No memories stored"
                                : `${Object.keys(memories).length} entr${Object.keys(memories).length === 1 ? "y" : "ies"}`}
                            </div>
                          </div>
                          <div className="p-2 bg-icon-rule/10 rounded-lg group-hover:bg-icon-rule/20 transition-colors shrink-0">
                            <Brain size={18} className="text-icon-rule" />
                          </div>
                        </div>
                      </button>
                    </div>

                    <div className="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
                    {/* Description + Directory */}
                    <section className="bg-bg-input border border-border-strong/40 rounded-lg p-5 space-y-4">
                      <div>
                        <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                          Description
                        </label>
                        <textarea
                          value={project.description}
                          onChange={(e) => updateField("description", e.target.value)}
                          placeholder="What is this project for?"
                          rows={3}
                          className="w-full bg-bg-input border border-border-strong hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none resize-none transition-colors"
                        />
                      </div>
                      <div>
                        <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                          <span className="flex items-center gap-1.5">
                            <FolderOpen size={12} /> Project Directory
                          </span>
                        </label>
                        <div className="flex gap-2">
                          <input
                            type="text"
                            value={project.directory}
                            onChange={(e) => updateField("directory", e.target.value)}
                            placeholder="/path/to/your/project"
                            className="flex-1 bg-bg-base border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors"
                          />
                          <button
                            onClick={async () => {
                              const selected = await open({
                                directory: true,
                                multiple: false,
                                title: "Select project directory",
                              });
                              if (selected) {
                                updateField("directory", selected as string);
                              }
                            }}
                            className="px-3 py-2 bg-bg-input hover:bg-surface-hover text-text-base text-[12px] font-medium rounded border border-border-strong transition-colors shadow-sm whitespace-nowrap"
                          >
                            Browse
                          </button>
                        </div>
                        <p className="mt-1.5 text-[11px] text-text-muted">
                          Agent configs will be written to this directory when you sync.
                        </p>
                      </div>
                    </section>

                      {/* Quick Actions */}
                      <div className="flex flex-col gap-3">

                       {/* Apply Project Template */}
                       {availableProjectTemplates.length > 0 && (
                         <div>
                           <button
                             onClick={() => setShowProjectTemplatePicker(!showProjectTemplatePicker)}
                             className="w-full group flex items-center gap-3 bg-bg-input border border-border-strong/40 hover:border-brand/50 rounded-lg p-4 transition-all hover:shadow-lg hover:shadow-brand/10 text-left"
                           >
                             <div className="p-2 bg-brand/10 rounded-lg group-hover:bg-brand/20 transition-colors">
                               <LayoutTemplate size={16} className="text-brand" />
                             </div>
                             <div className="flex-1 min-w-0">
                               <div className="text-[13px] font-medium text-text-base mb-0.5">Apply Project Template</div>
                               <div className="text-[11px] text-text-muted">Merge agents, skills & servers from a template</div>
                             </div>
                             <ArrowRight size={14} className="text-text-muted opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                           </button>
                           {showProjectTemplatePicker && (
                              <div className="mt-1.5 p-2 bg-bg-input border border-border-strong/40 rounded-lg space-y-1">
                                {availableProjectTemplates.map((tmpl) => {
                                  const isSelected = selectedProjectTemplate === tmpl.name;
                                  return (
                                    <button
                                      key={tmpl.name}
                                      onClick={() => applyProjectTemplate(tmpl)}
                                      className={`w-full text-left px-3 py-2 rounded-md transition-colors flex items-start gap-2 ${
                                        isSelected
                                          ? "bg-brand/15 border border-brand/40"
                                          : "hover:bg-bg-sidebar border border-transparent"
                                      }`}
                                    >
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-text-base">{tmpl.name}</div>
                                        {tmpl.description && (
                                          <div className="text-[11px] text-text-muted mt-0.5 truncate">{tmpl.description}</div>
                                        )}
                                        <div className="flex items-center gap-3 mt-1">
                                          {tmpl.agents.length > 0 && (
                                            <span className="text-[10px] text-text-muted flex items-center gap-1">
                                              <Bot size={10} /> {tmpl.agents.length} agent{tmpl.agents.length !== 1 ? "s" : ""}
                                            </span>
                                          )}
                                          {tmpl.skills.length > 0 && (
                                            <span className="text-[10px] text-text-muted flex items-center gap-1">
                                              <Code size={10} /> {tmpl.skills.length} skill{tmpl.skills.length !== 1 ? "s" : ""}
                                            </span>
                                          )}
                                          {tmpl.mcp_servers.length > 0 && (
                                            <span className="text-[10px] text-text-muted flex items-center gap-1">
                                              <Server size={10} /> {tmpl.mcp_servers.length} MCP server{tmpl.mcp_servers.length !== 1 ? "s" : ""}
                                            </span>
                                          )}
                                        </div>
                                      </div>
                                      {isSelected && (
                                        <Check size={13} className="text-brand flex-shrink-0 mt-0.5" />
                                      )}
                                    </button>
                                  );
                                })}
                                 <div className="mt-1 flex items-center gap-2 px-3 py-1">
                                   <button
                                     onClick={() => setShowProjectTemplatePicker(false)}
                                     className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                                   >
                                     Cancel
                                   </button>
                                   {selectedProjectTemplate && (
                                     <>
                                       <span className="text-surface">·</span>
                                       <button
                                         onClick={() => {
                                           setSelectedProjectTemplate(null);
                                           setShowProjectTemplatePicker(false);
                                         }}
                                         className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                                       >
                                         Clear selection
                                       </button>
                                     </>
                                   )}
                                 </div>
                              </div>
                            )}
                         </div>
                       )}

                           {/* Open in editor card */}
                          <div className={`bg-bg-input border border-border-strong/40 rounded-lg p-4 transition-all ${project.directory ? "" : "opacity-40"}`}>
                            <div className="flex items-center gap-3 mb-3">
                              <div className="p-2 bg-brand/10 rounded-lg">
                                <FolderOpen size={16} className="text-brand" />
                              </div>
                              <div className="flex-1 min-w-0">
                                <div className="text-[13px] font-medium text-text-base mb-0.5">Open in</div>
                                <div className="text-[11px] text-text-muted">
                                  {project.directory ? "Choose editor or Finder" : "No directory set"}
                                </div>
                              </div>
                            </div>
                            {project.directory && installedEditors.filter((e) => e.installed).length > 0 && (
                              <div className="flex flex-wrap gap-1.5">
                                {installedEditors.filter((e) => e.installed).map((editor) => (
                                  <button
                                    key={editor.id}
                                    onClick={() => handleOpenInEditor(editor.id)}
                                    title={`Open in ${editor.label}`}
                                    className="flex items-center gap-1.5 px-2 py-1 bg-bg-sidebar hover:bg-surface-hover border border-border-strong/40 hover:border-border-strong rounded text-[11px] text-text-base transition-colors"
                                  >
                                    <EditorIcon id={editor.id} iconPath={editorIconPaths[editor.id]} />
                                    {editor.label}
                                  </button>
                                ))}
                                <button
                                  onClick={() => handleOpenInEditor("copy_path")}
                                  title="Copy project path"
                                  className="flex items-center gap-1.5 px-2 py-1 bg-bg-sidebar hover:bg-surface-hover border border-border-strong/40 hover:border-border-strong rounded text-[11px] text-text-muted hover:text-text-base transition-colors"
                                >
                                  <Copy size={11} />
                                  Copy path
                                </button>
                              </div>
                            )}
                          </div>

                         {/* Force Refresh */}
                         <button
                           onClick={async () => {
                             if (selectedName) {
                               await reloadProject(selectedName);
                             }
                           }}
                           className="group flex items-center gap-3 bg-bg-input border border-border-strong/40 hover:border-success/50 rounded-lg p-4 transition-all hover:shadow-lg hover:shadow-success/10 text-left"
                         >
                           <div className="p-2 bg-success/10 rounded-lg group-hover:bg-success/20 transition-colors">
                             <RotateCcw size={16} className="text-success" />
                           </div>
                           <div className="flex-1 min-w-0">
                             <div className="text-[13px] font-medium text-text-base mb-0.5">Force Refresh</div>
                             <div className="text-[11px] text-text-muted">Reload project from disk</div>
                           </div>
                           <ArrowRight size={14} className="text-text-muted opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                         </button>
                         </div>
                     </div>


                    {/* Getting Started (existing saved project that is still incomplete) */}
                    {!isCreating && (!project.directory || project.agents.length === 0) && (
                      <section className="bg-gradient-to-br from-brand/10 to-brand/5 border border-brand/20 rounded-lg p-5">
                        <div className="flex items-start gap-3">
                          <div className="p-2 bg-brand/20 rounded-lg flex-shrink-0">
                            <Package size={18} className="text-brand" />
                          </div>
                          <div>
                            <h3 className="text-[13px] font-semibold text-text-base mb-2">Complete Setup</h3>
                            <p className="text-[12px] text-text-muted mb-3 leading-relaxed">
                              To start using this project, complete these steps:
                            </p>
                            <ol className="space-y-2 text-[12px] text-text-base">
                              {!project.directory && (
                                <li className="flex items-start gap-2">
                                  <div className="w-5 h-5 rounded-full border border-brand flex items-center justify-center flex-shrink-0 mt-0.5">
                                    <span className="text-[10px] text-brand">1</span>
                                  </div>
                                  <div>
                                     <span className="text-text-base font-medium">
                                       Set project directory
                                     </span>
                                    <div className="text-[11px] text-text-muted mt-0.5">
                                      Choose where agent configs will be synced
                                    </div>
                                  </div>
                                </li>
                              )}
                              {project.agents.length === 0 && (
                                <li className="flex items-start gap-2">
                                  <div className="w-5 h-5 rounded-full border border-brand flex items-center justify-center flex-shrink-0 mt-0.5">
                                    <span className="text-[10px] text-brand">{!project.directory ? "2" : "1"}</span>
                                  </div>
                                  <div>
                                    <button
                                      onClick={() => setProjectTab("agents")}
                                      className="text-brand hover:text-brand-hover transition-colors font-medium"
                                    >
                                      Add agent tools
                                    </button>
                                    <div className="text-[11px] text-text-muted mt-0.5">
                                      Select which agents will use this project
                                    </div>
                                  </div>
                                </li>
                              )}
                              <li className="flex items-start gap-2">
                                <div className="w-5 h-5 rounded-full border border-text-muted/50 flex items-center justify-center flex-shrink-0 mt-0.5">
                                  <span className="text-[10px] text-text-muted">•</span>
                                </div>
                                <div>
                                  <button
                                    onClick={() => setProjectTab("skills")}
                                    className="text-text-base hover:text-brand transition-colors"
                                  >
                                    Add skills (optional)
                                  </button>
                                  <div className="text-[11px] text-text-muted mt-0.5">
                                    Give agents specialized capabilities
                                  </div>
                                </div>
                              </li>
                            </ol>
                          </div>
                        </div>
                      </section>
                    )}
                  </>
                )}

                {/* ── Details tab ──────────────────────────────────────── */}
                 {/* ── Agents tab ───────────────────────────────────────── */}
                {projectTab === "agents" && (
                  <section>
                     <AgentSelector
                       agentIds={project.agents}
                       availableAgents={availableAgents}
                       onAdd={(id) => addItem("agents", id)}
                       onRemove={(i) => handleRemoveAgent(i)}
                       emptyMessage="No agent tools selected. Add tools to enable config sync."
                     />
                  </section>
                )}

                {/* ── Skills tab ───────────────────────────────────────── */}
                {projectTab === "skills" && (
                  <>
                    {/* Global Skills */}
                     <section>
                       <SkillSelector
                         skills={project.skills}
                         availableSkills={availableSkills}
                         onAdd={(s) => addItem("skills", s)}
                         onRemove={(i) => removeItem("skills", i)}
                         emptyMessage="No skills attached."
                       />
                     </section>

                    {/* Local Skills */}
                    {project.local_skills.length > 0 && (
                      <section>
                        <div className="flex items-center justify-between mb-2">
                          <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase flex items-center gap-1.5">
                            <Code size={12} /> Local Skills
                          </label>
                          {project.local_skills.length > 1 && selectedName && (
                            <button
                              onClick={async () => {
                                try {
                                  setSyncStatus("syncing");
                                  await invoke("sync_local_skills", { name: selectedName });
                                  setSyncStatus("Local skills synced across agents");
                                  setTimeout(() => setSyncStatus(null), 4000);
                                } catch (err: any) {
                                  setSyncStatus(`Sync failed: ${err}`);
                                  setTimeout(() => setSyncStatus(null), 4000);
                                }
                              }}
                              className="flex items-center gap-1 text-[11px] text-text-muted hover:text-text-base px-1.5 py-0.5 hover:bg-bg-sidebar rounded transition-colors"
                              title="Copy all local skills to every agent's skill directory"
                            >
                              <ArrowRightLeft size={11} /> Sync Across Agents
                            </button>
                          )}
                        </div>
                        <p className="text-[11px] text-text-muted mb-2">
                          These skills exist only in this project directory, not in the global registry.
                        </p>
                        <ul className="space-y-1">
                          {project.local_skills.map((s) => (
                            <li
                              key={s}
                              className={`group flex flex-col bg-bg-input rounded-md border text-[13px] transition-colors ${
                                localSkillEditing === s ? "border-brand/50" : "border-border-strong/40"
                              }`}
                            >
                              {/* Row: name + action buttons */}
                              <div className="flex items-center justify-between px-3 py-1.5">
                                <button
                                  onClick={async () => {
                                    if (!selectedName) return;
                                    if (localSkillEditing === s) {
                                      // Collapse if already open and not actively editing
                                      if (!localSkillIsEditing) {
                                        setLocalSkillEditing(null);
                                        setLocalSkillContent("");
                                      }
                                      return;
                                    }
                                    // Load and expand
                                    try {
                                      const content: string = await invoke("read_local_skill", { name: selectedName, skillName: s });
                                      const { meta, body } = parseLocalSkillFrontmatter(content);
                                      setLocalSkillEditing(s);
                                      setLocalSkillContent(content);
                                      setLocalSkillEditName(meta.name ?? s);
                                      setLocalSkillEditDescription(meta.description ?? "");
                                      setLocalSkillEditBody(body);
                                      setLocalSkillFieldErrors({ name: null, description: null });
                                      setLocalSkillIsEditing(false);
                                    } catch (err: any) {
                                      setSyncStatus(`Failed to read skill: ${err}`);
                                      setTimeout(() => setSyncStatus(null), 4000);
                                    }
                                  }}
                                  className="flex items-center gap-2 flex-1 text-left text-text-base hover:text-text-base"
                                >
                                  <Code size={12} className="text-text-muted" />
                                  <span>{s}</span>
                                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar text-text-muted border border-border-strong/40">
                                    local
                                  </span>
                                  {/* Chevron */}
                                  <svg
                                    width="10" height="10" viewBox="0 0 10 10" fill="currentColor"
                                    className={`ml-auto text-text-muted transition-transform ${localSkillEditing === s ? "rotate-90" : ""}`}
                                  >
                                    <path d="M3 2l4 3-4 3V2z"/>
                                  </svg>
                                </button>
                                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all ml-2">
                                  <button
                                    onClick={async () => {
                                      if (!selectedName) return;
                                      // Load and switch directly to edit mode
                                      try {
                                        const content: string = await invoke("read_local_skill", { name: selectedName, skillName: s });
                                        const { meta, body } = parseLocalSkillFrontmatter(content);
                                        setLocalSkillEditing(s);
                                        setLocalSkillContent(content);
                                        setLocalSkillEditName(meta.name ?? s);
                                        setLocalSkillEditDescription(meta.description ?? "");
                                        setLocalSkillEditBody(body);
                                        setLocalSkillFieldErrors({ name: null, description: null });
                                        setLocalSkillIsEditing(true);
                                      } catch (err: any) {
                                        setSyncStatus(`Failed to read skill: ${err}`);
                                        setTimeout(() => setSyncStatus(null), 4000);
                                      }
                                    }}
                                    className="text-text-muted hover:text-text-base p-1 hover:bg-bg-sidebar rounded transition-colors"
                                    title="Edit skill"
                                  >
                                    <Edit2 size={12} />
                                  </button>
                                  <button
                                    onClick={async () => {
                                      if (!selectedName) return;
                                      try {
                                        setSyncStatus("syncing");
                                        await invoke("sync_local_skills", { name: selectedName });
                                        setSyncStatus(`Synced "${s}" across agents`);
                                        setTimeout(() => setSyncStatus(null), 4000);
                                      } catch (err: any) {
                                        setSyncStatus(`Sync failed: ${err}`);
                                        setTimeout(() => setSyncStatus(null), 4000);
                                      }
                                    }}
                                    className="text-text-muted hover:text-text-base p-1 hover:bg-bg-sidebar rounded transition-colors"
                                    title="Sync to all agents in this project"
                                  >
                                    <ArrowRightLeft size={12} />
                                  </button>
                                  <button
                                    onClick={async () => {
                                      if (!selectedName) return;
                                      try {
                                        setSyncStatus("syncing");
                                        const result: string = await invoke("import_local_skill", { name: selectedName, skillName: s });
                                        const updated = JSON.parse(result);
                                        setProject({
                                          ...project,
                                          skills: updated.skills || project.skills,
                                          local_skills: updated.local_skills || [],
                                        });
                                        if (localSkillEditing === s) {
                                          setLocalSkillEditing(null);
                                          setLocalSkillContent("");
                                        }
                                        await loadAvailableSkills();
                                        setSyncStatus(`Imported "${s}" to global registry`);
                                        setTimeout(() => setSyncStatus(null), 4000);
                                      } catch (err: any) {
                                        setSyncStatus(`Import failed: ${err}`);
                                        setTimeout(() => setSyncStatus(null), 4000);
                                      }
                                    }}
                                    className="text-text-muted hover:text-success p-1 hover:bg-bg-sidebar rounded transition-colors"
                                    title="Import to global skill registry"
                                  >
                                    <Upload size={12} />
                                  </button>
                                </div>
                              </div>

                              {/* Expanded editor panel */}
                              {localSkillEditing === s && (
                                <div className="border-t border-border-strong/40 px-3 pt-3 pb-3 space-y-3">

                                  {/* Editor header */}
                                  <div className="flex items-center justify-between">
                                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase flex items-center gap-1.5">
                                      <FileText size={11} /> Edit Skill
                                    </span>
                                    <div className="flex items-center gap-1.5">
                                      {localSkillIsEditing ? (
                                        <>
                                          <button
                                            onClick={() => {
                                              // Revert to last loaded content
                                              const { meta, body } = parseLocalSkillFrontmatter(localSkillContent);
                                              setLocalSkillEditName(meta.name ?? s);
                                              setLocalSkillEditDescription(meta.description ?? "");
                                              setLocalSkillEditBody(body);
                                              setLocalSkillFieldErrors({ name: null, description: null });
                                              setLocalSkillIsEditing(false);
                                            }}
                                            className="px-2 py-1 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[11px] font-medium transition-colors"
                                          >
                                            Cancel
                                          </button>
                                          <button
                                            disabled={localSkillSaving}
                                            onClick={async () => {
                                              if (!selectedName) return;
                                              const nameErr = validateLocalSkillName(localSkillEditName);
                                              const descErr = validateLocalSkillDescription(localSkillEditDescription);
                                              setLocalSkillFieldErrors({ name: nameErr, description: descErr });
                                              if (nameErr || descErr) return;
                                              const finalContent = buildLocalSkillFrontmatter(localSkillEditName, localSkillEditDescription) + "\n" + localSkillEditBody;
                                              setLocalSkillSaving(true);
                                              try {
                                                await invoke("save_local_skill", { name: selectedName, skillName: s, content: finalContent });
                                                setLocalSkillContent(finalContent);
                                                setLocalSkillIsEditing(false);
                                                setSyncStatus(`Saved "${s}"`);
                                                setTimeout(() => setSyncStatus(null), 3000);
                                              } catch (err: any) {
                                                setSyncStatus(`Save failed: ${err}`);
                                                setTimeout(() => setSyncStatus(null), 4000);
                                              } finally {
                                                setLocalSkillSaving(false);
                                              }
                                            }}
                                            className="flex items-center gap-1 px-2 py-1 bg-brand hover:bg-brand-hover text-white rounded text-[11px] font-medium transition-colors disabled:opacity-50"
                                          >
                                            <Check size={11} /> Save
                                          </button>
                                        </>
                                      ) : (
                                        <button
                                          onClick={() => setLocalSkillIsEditing(true)}
                                          className="flex items-center gap-1 px-2 py-1 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[11px] font-medium transition-colors"
                                        >
                                          <Edit2 size={11} /> Edit
                                        </button>
                                      )}
                                    </div>
                                  </div>

                                  {localSkillIsEditing ? (
                                    /* Edit form */
                                    <div className="space-y-3">
                                      {/* Name field */}
                                      <div>
                                        <div className="flex items-baseline justify-between mb-1">
                                          <label className="text-[10px] font-semibold text-text-muted tracking-wider uppercase">
                                            Name <span className="text-red-400 ml-0.5">*</span>
                                          </label>
                                          <span className={`text-[10px] tabular-nums ${localSkillEditName.length > 58 ? (localSkillEditName.length > 64 ? "text-red-400" : "text-warning") : "text-text-muted"}`}>
                                            {localSkillEditName.length}/64
                                          </span>
                                        </div>
                                        <input
                                          type="text"
                                          value={localSkillEditName}
                                          onChange={(e) => {
                                            const raw = e.target.value.toLowerCase().replace(/\s+/g, "-").replace(/[^a-z0-9-]/g, "");
                                            setLocalSkillEditName(raw);
                                            setLocalSkillFieldErrors(prev => ({ ...prev, name: validateLocalSkillName(raw) }));
                                          }}
                                          maxLength={64}
                                          className={`w-full px-2.5 py-1.5 rounded-md bg-bg-sidebar border outline-none text-[12px] text-text-base placeholder-text-muted/40 font-mono transition-colors ${
                                            localSkillFieldErrors.name ? "border-red-500/60 focus:border-red-500" : "border-border-strong/40 hover:border-border-strong focus:border-brand"
                                          }`}
                                          spellCheck={false}
                                        />
                                        {localSkillFieldErrors.name && (
                                          <p className="mt-1 text-[10px] text-red-400">{localSkillFieldErrors.name}</p>
                                        )}
                                      </div>

                                      {/* Description field */}
                                      <div>
                                        <div className="flex items-baseline justify-between mb-1">
                                          <label className="text-[10px] font-semibold text-text-muted tracking-wider uppercase">
                                            Description <span className="text-red-400 ml-0.5">*</span>
                                          </label>
                                          <span className={`text-[10px] tabular-nums ${localSkillEditDescription.length > 900 ? (localSkillEditDescription.length > 1024 ? "text-red-400" : "text-warning") : "text-text-muted"}`}>
                                            {localSkillEditDescription.length}/1024
                                          </span>
                                        </div>
                                        <textarea
                                          value={localSkillEditDescription}
                                          onChange={(e) => {
                                            setLocalSkillEditDescription(e.target.value);
                                            setLocalSkillFieldErrors(prev => ({ ...prev, description: validateLocalSkillDescription(e.target.value) }));
                                          }}
                                          rows={2}
                                          maxLength={1024}
                                          className={`w-full px-2.5 py-1.5 rounded-md bg-bg-sidebar border outline-none text-[12px] text-text-base placeholder-text-muted/40 resize-none transition-colors leading-relaxed ${
                                            localSkillFieldErrors.description ? "border-red-500/60 focus:border-red-500" : "border-border-strong/40 hover:border-border-strong focus:border-brand"
                                          }`}
                                          spellCheck={false}
                                        />
                                        {localSkillFieldErrors.description && (
                                          <p className="mt-1 text-[10px] text-red-400">{localSkillFieldErrors.description}</p>
                                        )}
                                      </div>

                                      {/* Body field */}
                                      <div>
                                        <label className="text-[10px] font-semibold text-text-muted tracking-wider uppercase block mb-1">
                                          Body
                                        </label>
                                        <textarea
                                          value={localSkillEditBody}
                                          onChange={(e) => setLocalSkillEditBody(e.target.value)}
                                          rows={12}
                                          className="w-full px-2.5 py-1.5 rounded-md bg-bg-sidebar border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[12px] text-text-base font-mono resize-y transition-colors leading-relaxed custom-scrollbar"
                                          spellCheck={false}
                                        />
                                      </div>
                                    </div>
                                  ) : (
                                    /* Preview */
                                    <div className="rounded-md bg-bg-sidebar/40 border border-border-strong/40 px-3 py-2.5">
                                      {localSkillEditName && (
                                        <p className="text-[13px] font-medium text-text-base mb-1">{localSkillEditName}</p>
                                      )}
                                      {localSkillEditDescription && (
                                        <p className="text-[12px] text-text-muted leading-relaxed mb-2">{localSkillEditDescription}</p>
                                      )}
                                      {localSkillEditBody ? (
                                        <MarkdownPreview content={localSkillEditBody} />
                                      ) : (
                                        <p className="text-[12px] text-text-muted italic">No body content. Click Edit to add instructions.</p>
                                      )}
                                    </div>
                                  )}
                                </div>
                              )}
                            </li>
                          ))}
                        </ul>
                      </section>
                    )}
                  </>
                )}

                {/* ── MCP Servers tab ──────────────────────────────────── */}
                {projectTab === "mcp_servers" && (() => {
                  const hasWarp = project.agents.includes("warp");
                  const warpOnly = hasWarp && project.agents.length === 1;
                  const warpNote = availableAgents.find((a) => a.id === "warp")?.mcp_note ?? null;
                  return (
                  <section>
                    {/* Warp-only: MCP config not available */}
                    {warpOnly && warpNote && (
                      <div className="mb-4 flex items-start gap-3 px-4 py-3 bg-bg-input border border-border-strong rounded-lg">
                        <AlertCircle size={15} className="text-text-muted flex-shrink-0 mt-0.5" />
                        <div>
                          <p className="text-[13px] font-medium text-text-base mb-0.5">MCP not configurable via Automatic</p>
                          <p className="text-[12px] text-text-muted leading-relaxed">{warpNote}</p>
                        </div>
                      </div>
                    )}

                    {/* Warp + other agents: partial warning */}
                    {hasWarp && !warpOnly && warpNote && (
                      <div className="mb-4 flex items-start gap-3 px-4 py-3 bg-warning/8 border border-warning/30 rounded-lg">
                        <AlertCircle size={15} className="text-warning flex-shrink-0 mt-0.5" />
                        <div>
                          <p className="text-[13px] font-medium text-warning mb-0.5">Warp requires manual MCP setup</p>
                          <p className="text-[12px] text-warning/80 leading-relaxed">{warpNote}</p>
                        </div>
                      </div>
                    )}

                    <McpSelector
                      servers={project.mcp_servers}
                      availableServers={availableMcpServers}
                      onAdd={(s) => addItem("mcp_servers", s)}
                      onRemove={(i) => removeItem("mcp_servers", i)}
                      disableAdd={warpOnly}
                      emptyMessage={warpOnly ? "Add other agent tools to enable MCP server syncing." : "No MCP servers attached."}
                    />
                  </section>
                  );
                })()}

                {/* ── Memory tab ──────────────────────────────────── */}
                {projectTab === "memory" && (
                  <section>
                    <div className="flex items-center justify-between mb-4">
                      <div>
                        <h3 className="text-[14px] font-medium text-text-base">Agent Memory</h3>
                        <p className="text-[12px] text-text-muted mt-1">
                          Persistent context and learnings stored by agents working on this project.
                        </p>
                      </div>
                      {Object.keys(memories).length > 0 && (
                        <button
                          onClick={async () => {
                            if (!selectedName || !(await ask("Are you sure you want to clear all memory for this project? This cannot be undone.", { title: "Clear Memories", kind: "warning" }))) return;
                            try {
                              await invoke("clear_memories", { project: selectedName, confirm: true, pattern: null });
                              await loadMemories(selectedName);
                            } catch (err: any) {
                              setError(`Failed to clear memories: ${err}`);
                            }
                          }}
                          className="flex items-center gap-1.5 px-3 py-1.5 bg-danger/10 hover:bg-danger/20 text-danger rounded text-[12px] font-medium border border-danger/20 transition-colors"
                        >
                          <Trash2 size={12} /> Clear All
                        </button>
                      )}
                    </div>

                    {loadingMemories ? (
                      <div className="text-[13px] text-text-muted text-center py-8">Loading memories...</div>
                    ) : Object.keys(memories).length === 0 ? (
                      <div className="text-center py-12 bg-bg-input rounded-lg border border-border-strong/40 border-dashed">
                        <div className="w-12 h-12 mx-auto mb-3 rounded-full bg-bg-sidebar flex items-center justify-center">
                          <Bot size={20} className="text-text-muted" />
                        </div>
                        <h4 className="text-[13px] font-medium text-text-base mb-1">No memories yet</h4>
                        <p className="text-[12px] text-text-muted max-w-sm mx-auto">
                          Agents haven't stored any learnings or context for this project yet.
                        </p>
                      </div>
                    ) : (
                      <div className="space-y-3">
                        {Object.entries(memories).map(([key, memory]) => {
                          const isEditing = editingMemoryKey === key;
                          const isCopied = copiedMemoryKey === key;
                          return (
                          <div key={key} className="bg-bg-input border border-border-strong/40 rounded-lg p-4 group">
                            <div className="flex items-start justify-between gap-4 mb-2">
                              <div className="min-w-0 flex-1">
                                <div className="flex items-center gap-2 mb-1">
                                  <h4 className="text-[13px] font-semibold text-text-base font-mono truncate">{key}</h4>
                                  {memory.source && (
                                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar text-text-muted border border-border-strong/40">
                                      {memory.source}
                                    </span>
                                  )}
                                </div>
                                <p className="text-[11px] text-text-muted">
                                  {new Date(memory.timestamp).toLocaleString()}
                                </p>
                              </div>
                              <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                <button
                                  onClick={() => {
                                    navigator.clipboard.writeText(memory.value);
                                    setCopiedMemoryKey(key);
                                    setTimeout(() => setCopiedMemoryKey(null), 1500);
                                  }}
                                  className="text-text-muted hover:text-text-base p-1.5 hover:bg-surface rounded transition-colors"
                                  title="Copy value"
                                >
                                  {isCopied ? <Check size={14} className="text-success" /> : <Copy size={14} />}
                                </button>
                                <button
                                  onClick={() => {
                                    if (isEditing) {
                                      setEditingMemoryKey(null);
                                      setEditingMemoryValue("");
                                    } else {
                                      setEditingMemoryKey(key);
                                      setEditingMemoryValue(memory.value);
                                    }
                                  }}
                                  className={`p-1.5 rounded transition-colors ${isEditing ? "text-brand bg-brand/10 hover:bg-brand/20" : "text-text-muted hover:text-text-base hover:bg-surface"}`}
                                  title={isEditing ? "Cancel edit" : "Edit memory"}
                                >
                                  <Edit2 size={14} />
                                </button>
                                <button
                                  onClick={async () => {
                                    if (!selectedName) return;
                                    try {
                                      await invoke("delete_memory", { project: selectedName, key });
                                      if (editingMemoryKey === key) {
                                        setEditingMemoryKey(null);
                                        setEditingMemoryValue("");
                                      }
                                      await loadMemories(selectedName);
                                    } catch (err: any) {
                                      setError(`Failed to delete memory: ${err}`);
                                    }
                                  }}
                                  className="text-text-muted hover:text-danger p-1.5 hover:bg-surface rounded transition-colors"
                                  title="Delete memory"
                                >
                                  <Trash2 size={14} />
                                </button>
                              </div>
                            </div>
                            {isEditing ? (
                              <div className="space-y-2">
                                <textarea
                                  value={editingMemoryValue}
                                  onChange={(e) => setEditingMemoryValue(e.target.value)}
                                  className="w-full text-[13px] text-text-base font-mono bg-bg-base p-3 rounded border border-brand resize-none focus:outline-none focus:ring-1 focus:ring-brand custom-scrollbar"
                                  rows={Math.min(Math.max(editingMemoryValue.split("\n").length + 1, 4), 15)}
                                  autoFocus
                                />
                                <div className="flex items-center justify-end gap-2">
                                  <button
                                    onClick={() => {
                                      setEditingMemoryKey(null);
                                      setEditingMemoryValue("");
                                    }}
                                    className="px-3 py-1 text-[12px] font-medium text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
                                  >
                                    Cancel
                                  </button>
                                  <button
                                    disabled={savingMemory || editingMemoryValue.trim() === ""}
                                    onClick={async () => {
                                      if (!selectedName) return;
                                      try {
                                        setSavingMemory(true);
                                        await invoke("store_memory", {
                                          project: selectedName,
                                          key,
                                          value: editingMemoryValue,
                                          source: memory.source ?? null,
                                        });
                                        setEditingMemoryKey(null);
                                        setEditingMemoryValue("");
                                        await loadMemories(selectedName);
                                      } catch (err: any) {
                                        setError(`Failed to save memory: ${err}`);
                                      } finally {
                                        setSavingMemory(false);
                                      }
                                    }}
                                    className="px-3 py-1 text-[12px] font-medium bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-white rounded transition-colors"
                                  >
                                    {savingMemory ? "Saving..." : "Save"}
                                  </button>
                                </div>
                              </div>
                            ) : (
                              <div className="text-[13px] text-text-base whitespace-pre-wrap font-mono bg-bg-base p-3 rounded border border-border-strong/40 max-h-60 overflow-y-auto custom-scrollbar">
                                {memory.value}
                              </div>
                            )}
                          </div>
                          );
                        })}
                      </div>
                    )}
                  </section>
                )}

              </div>
            </div>
            )}
            </>}
          </div>
        ) : (
          /* Empty state */
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-16 h-16 mx-auto mb-6 rounded-full border border-dashed border-border-strong flex items-center justify-center text-text-muted">
              <FolderOpen size={24} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-text-base mb-2">
              No Project Selected
            </h2>
            <p className="text-[14px] text-text-muted mb-8 leading-relaxed max-w-sm">
              Projects group skills and MCP servers into reusable
              configurations. Select one from the sidebar or create a new
              project.
            </p>
            <button
              onClick={startCreate}
              className="px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              Create Project
            </button>
          </div>
        )}
      </div>
    </div>

    {/* ── Drift diff modal ─────────────────────────────────────────────── */}
    {driftDiffFile && (
      <DriftDiffModal
        file={driftDiffFile.file}
        agentLabel={driftDiffFile.agentLabel}
        onClose={() => setDriftDiffFile(null)}
      />
    )}
    </>
  );
}
