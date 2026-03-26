import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { getToolPanel } from "../../plugins";
import mcpServersData from "../../../src-tauri/assets/marketplace/featured-mcp-servers.json";
import { SkillSelector } from "../../components/SkillSelector";
import { AgentSelector } from "../../components/AgentSelector";
import type { AgentOptions } from "../../components/AgentSelector";
import { AgentIcon } from "../../components/AgentIcon";
import { McpSelector } from "../../components/McpSelector";
import { MarkdownPreview } from "../../components/MarkdownPreview";
import { TokenPill } from "../../components/TokenPill";
import { useCurrentUser } from "../../contexts/ProfileContext";
import { useTaskLog } from "../../contexts/TaskLogContext";
import { MemoryBrowser } from "../../components/MemoryBrowser";
import { ClaudeMemoryPanel } from "../../components/ClaudeMemoryPanel";
import Features, { type Feature } from "./Features";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { handleExternalLinkClick } from "../../lib/externalLinks";
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
} from "../../lib/analytics";

import {
  Plus,
  X,
  FolderOpen,
  FolderPlus,
  Folder,
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
  ChevronLeft,
  ChevronRight,
  Copy,
  History,
  Search,
  Sparkles,
  Lightbulb,
  Pin,
  PinOff,
  Link as LinkIcon,
  ExternalLink,
  Wrench,
  Terminal,
  CheckCircle2,
  MinusCircle,
  Globe,
  Puzzle,
  Layers,
  MessagesSquare,
} from "lucide-react";

interface CustomRule {
  name: string;
  content: string;
}

interface CustomAgent {
  name: string;
  content: string;
}

interface CustomCommand {
  name: string;
  content: string;
}

interface UserAgentEntry {
  id: string;
  name: string;
}

interface UserCommandEntry {
  id: string;
  description: string;
}

interface Project {
  name: string;
  description: string;
  directory: string;
  skills: string[];
  local_skills: string[];
  mcp_servers: string[];
  disabled_mcp_servers?: string[];
  providers: string[];
  agents: string[];
  created_at: string;
  updated_at: string;
  last_activity?: string;
  created_by?: string;
  file_rules?: Record<string, string[]>;
  instruction_mode?: string;
  /** Per-agent options keyed by agent id. Agents not present use defaults. */
  agent_options?: Record<string, AgentOptions>;
  /** Inline custom rules stored directly in this project (not in the global registry). */
  custom_rules?: CustomRule[];
  /** Inline custom agents stored directly in this project. Written to .claude/agents/ (or equivalent) on sync. */
  custom_agents?: CustomAgent[];
  /** Tool names detected as present in this project (populated by autodetect). */
  tools?: string[];
  /** Workspace agent names selected for this project. Written to agent's sub-agent directory on sync. */
  user_agents?: string[];
  /** Workspace command names selected for this project. Written to provider command directories on sync. */
  user_commands?: string[];
  /** Inline custom commands stored directly in this project. */
  custom_commands?: CustomCommand[];
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

interface InstructionFileConflict {
  /** The instruction filename (e.g. "AGENTS.md", "CLAUDE.md"). */
  filename: string;
  /** Agent labels that use this file. */
  agent_labels: string[];
  /** User-authored content currently on disk (managed sections stripped). */
  disk_content: string;
  /** User-authored content Automatic has stored (empty if never set through Automatic). */
  automatic_content: string;
}

interface RebuildPreviewCategory {
  key: string;
  label: string;
  automatic: string[];
  disk: string[];
  added: string[];
  removed: string[];
}

interface RebuildPreview {
  project_name: string;
  categories: RebuildPreviewCategory[];
  changed: boolean;
}

function parseInvokeResult<T>(value: unknown): T {
  if (typeof value === "string") {
    return JSON.parse(value) as T;
  }

  return value as T;
}

interface DriftReport {
  drifted: boolean;
  agents: AgentDrift[];
  /** Instruction files that have external content Automatic does not recognise. */
  instruction_conflicts?: InstructionFileConflict[];
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

// ── Project Folder types ──────────────────────────────────────────────────────

interface ProjectFolder {
  id: string;
  name: string;
  collapsed: boolean;
  projectNames: string[];
}

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

// ── Activity ──────────────────────────────────────────────────────────────────

interface ActivityEntry {
  id: number;
  project: string;
  event: string;
  label: string;
  detail: string;
  timestamp: string;
}

interface ProjectRecommendation {
  id: number;
  project: string;
  kind: string;
  title: string;
  body: string;
  priority: "low" | "normal" | "high";
  status: "pending" | "dismissed" | "actioned";
  source: string;
  /** Optional JSON blob with extra data, e.g. `{"id":"owner/repo/skill","name":"skill","source":"owner/repo","installs":0}` */
  metadata: string;
  created_at: string;
  updated_at: string;
}

/** Returns a relative time string ("just now", "5 min ago", "2 days ago", etc.) */
function relativeTime(iso: string): string {
  const now = Date.now();
  const then = new Date(iso).getTime();
  const diffMs = now - then;
  if (diffMs < 0) return "just now";
  const diffSec = Math.floor(diffMs / 1000);
  if (diffSec < 60) return "just now";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} min ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  const diffDay = Math.floor(diffHr / 24);
  if (diffDay === 1) return "Yesterday";
  if (diffDay < 7) return `${diffDay} days ago`;
  const diffWk = Math.floor(diffDay / 7);
  if (diffWk === 1) return "1 week ago";
  if (diffWk < 5) return `${diffWk} weeks ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Returns icon + dot colour for a given event kind */
function activityMeta(event: string): { icon: React.ReactNode; dot: string } {
  switch (event) {
    case "sync":
      return { icon: <Check size={12} className="text-success" />, dot: "bg-success" };
    case "skill_added":
      return { icon: <Code size={12} className="text-icon-skill" />, dot: "bg-icon-skill" };
    case "skill_removed":
      return { icon: <Code size={12} className="text-text-muted" />, dot: "bg-text-muted" };
    case "mcp_server_added":
      return { icon: <Server size={12} className="text-icon-mcp" />, dot: "bg-icon-mcp" };
    case "mcp_server_removed":
      return { icon: <Server size={12} className="text-text-muted" />, dot: "bg-text-muted" };
    case "agent_added":
      return { icon: <Bot size={12} className="text-brand" />, dot: "bg-brand" };
    case "agent_removed":
      return { icon: <Bot size={12} className="text-text-muted" />, dot: "bg-text-muted" };
    case "project_created":
      return { icon: <FolderOpen size={12} className="text-brand" />, dot: "bg-brand" };
    case "project_updated":
      return { icon: <RefreshCw size={12} className="text-text-muted" />, dot: "bg-text-muted" };
    default:
      return { icon: <History size={12} className="text-text-muted" />, dot: "bg-text-muted" };
  }
}

interface ActivityFeedProps {
  entries: ActivityEntry[];
  loading: boolean;
}

function ActivityFeed({ entries, loading }: ActivityFeedProps) {
  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <History size={13} className="text-text-muted" />
        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Recent Activity</span>
      </div>
      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
        {loading ? (
          <div className="px-4 py-6 text-center text-[12px] text-text-muted">Loading activity…</div>
        ) : entries.length === 0 ? (
          <div className="px-4 py-6 text-center text-[12px] text-text-muted italic">
            No activity yet. Save or sync the project to start recording events.
          </div>
        ) : (
          entries.map((item, i) => {
            const { icon, dot } = activityMeta(item.event);
            return (
              <div
                key={item.id}
                className={`flex items-center gap-3 px-4 py-2.5 ${i < entries.length - 1 ? "border-b border-border-strong/20" : ""}`}
              >
                <div className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${dot}`} />
                <div className="flex-shrink-0 text-text-muted">{icon}</div>
                <div className="flex-1 min-w-0">
                  <span className="text-[12px] text-text-base">{item.label}</span>
                  {item.detail && (
                    <span className="text-[12px] text-text-muted ml-1.5">{item.detail}</span>
                  )}
                </div>
                <span className="text-[11px] text-text-muted flex-shrink-0">{relativeTime(item.timestamp)}</span>
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

interface SummaryMetricCardProps {
  icon: React.ReactNode;
  label: string;
  count: number;
  accentClass: string;
  onView: () => void;
}

function SummaryMetricCard({ icon, label, count, accentClass, onView }: SummaryMetricCardProps) {
  return (
    <section
      role="button"
      tabIndex={0}
      onClick={onView}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onView(); } }}
      className="cursor-pointer rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3 transition-colors hover:border-border-strong hover:bg-bg-input/80"
    >
      <div className="flex items-center gap-2">
        <div className={`shrink-0 rounded-md p-1.5 ${accentClass}`}>{icon}</div>
        <span className="truncate text-[13px] font-semibold text-text-base">{label}</span>
      </div>
      <div className="mt-3 flex items-center gap-2">
        <span className="text-[24px] font-semibold leading-none tabular-nums text-text-base">{count}</span>
        <span className="rounded-full border border-border-strong/40 bg-bg-sidebar px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-text-muted">
          total
        </span>
      </div>
    </section>
  );
}

function SummarySidebarSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3">
      <div className="mb-3 text-[13px] font-semibold text-text-base">{title}</div>
      <div className="space-y-2">{children}</div>
    </section>
  );
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
    disabled_mcp_servers: [],
    providers: [],
    agents: [],
    user_agents: [],
    user_commands: [],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    file_rules: {},
    instruction_mode: "per-agent",
    custom_commands: [],
  };
}

interface ProjectsProps {
  /** Increment to navigate back to the projects list (deselects any open project). */
  resetKey?: number;
  initialProject?: string | null;
  onInitialProjectConsumed?: () => void;
  /** When set, switch to this project tab immediately after selecting the project. */
  initialProjectTab?: string | null;
  onInitialProjectTabConsumed?: () => void;
  /** Called when the user clicks "View in library" on a skill — navigates to the Skills page. */
  onNavigateToSkill?: (skillName: string) => void;
  /** Called when the user clicks "View full configuration" on an MCP server — navigates to the MCP Servers page. */
  onNavigateToMcpServer?: (serverName: string) => void;
  /** Called when the user clicks "View" on an AI-suggested skill — navigates to the Skill Store. */
  onNavigateToSkillStore?: (skillId: string) => void;
  /** Called when the user clicks "View" on an AI-suggested skill that has full metadata (id, name, source, installs).
   *  Navigates to the Skill Store and auto-selects the exact skill. */
  onNavigateToSkillStoreWithResult?: (result: { id: string; name: string; source: string; installs: number }) => void;
  /** Called when the user clicks "View" on an AI-suggested MCP server — navigates to the MCP Marketplace. */
  onNavigateToMcpMarketplace?: (slug: string) => void;
  /** Called when the user clicks on a project group name — navigates to the Project Groups page. */
  onNavigateToGroup?: (groupName: string) => void;
  /** When set, opens the new project wizard at step 3 with this template pre-selected. */
  initialCreateWithTemplate?: string | null;
  onInitialCreateWithTemplateConsumed?: () => void;
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
    // ── JetBrains IDEs ─────────────────────────────────────────────
    // All use the JetBrains icon pattern: gradient bg + black inset with abbreviation
    case "intellij":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="ij-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#087CFA"/>
              <stop offset="100%" stopColor="#FE315D"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#ij-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">IJ</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "phpstorm":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="ps-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#B345F1"/>
              <stop offset="100%" stopColor="#765AF8"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#ps-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">PS</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "webstorm":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="ws-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#07C3F2"/>
              <stop offset="100%" stopColor="#087CFA"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#ws-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="12" y="32" fontSize="14" fontWeight="700" fill="#fff" fontFamily="sans-serif">WS</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "pycharm":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="pc-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#21D789"/>
              <stop offset="100%" stopColor="#FCF84A"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#pc-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">PC</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "rustrover":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="rr-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#F26522"/>
              <stop offset="100%" stopColor="#FDB811"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#rr-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">RR</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "clion":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="cl-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#21D789"/>
              <stop offset="100%" stopColor="#009AE5"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#cl-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">CL</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "goland":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="gl-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#087CFA"/>
              <stop offset="100%" stopColor="#765AF8"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#gl-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">GL</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "datagrip":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="dg-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#22D88F"/>
              <stop offset="100%" stopColor="#9775F8"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#dg-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="12" y="32" fontSize="14" fontWeight="700" fill="#fff" fontFamily="sans-serif">DG</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
        </svg>
      );
    case "rider":
      return (
        <svg className={cls} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="rd-bg" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#C90F5E"/>
              <stop offset="100%" stopColor="#087CFA"/>
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="14" fill="url(#rd-bg)"/>
          <rect x="10" y="10" width="30" height="30" rx="2" fill="#000"/>
          <text x="13" y="32" fontSize="16" fontWeight="700" fill="#fff" fontFamily="sans-serif">RD</text>
          <rect x="10" y="44" width="20" height="3" rx="1" fill="#fff"/>
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
  projectName?: string;
  onClose: () => void;
  onResolved?: () => void;
}

function DriftDiffModal({ file, agentLabel, projectName, onClose, onResolved }: DriftDiffModalProps) {
  const diffLines = file.expected != null && file.actual != null
    ? computeLineDiff(file.expected, file.actual)
    : null;

  const [actionInProgress, setActionInProgress] = useState<string | null>(null);

  // Extract skill name from a stale drift path like ".agents/skills/my-skill"
  // or ".claude/skills/my-skill".  The skill name is the last path segment.
  const staleSkillName = file.reason === "stale"
    ? file.path.split("/").pop() ?? null
    : null;

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const handleAdoptSkill = async () => {
    if (!projectName || !staleSkillName) return;
    setActionInProgress("adopt");
    try {
      await invoke("adopt_stale_skill", { name: projectName, skillName: staleSkillName });
      onResolved?.();
      onClose();
    } catch (err: any) {
      console.error("Failed to adopt stale skill:", err);
      setActionInProgress(null);
    }
  };

  const handleRemoveSkill = async () => {
    if (!projectName || !staleSkillName) return;
    setActionInProgress("remove");
    try {
      await invoke("remove_stale_skill", { name: projectName, skillName: staleSkillName });
      onResolved?.();
      onClose();
    } catch (err: any) {
      console.error("Failed to remove stale skill:", err);
      setActionInProgress(null);
    }
  };

  const handleSyncOverwrite = async () => {
    if (!projectName) return;
    setActionInProgress("overwrite");
    try {
      await invoke("sync_project", { name: projectName });
      onResolved?.();
      onClose();
    } catch (err: any) {
      console.error("Failed to sync project:", err);
      setActionInProgress(null);
    }
  };

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
          ) : file.reason === "stale" && file.actual ? (
            /* Stale skill with on-disk content: show a header + content preview */
            <div className="flex flex-col h-full">
              <div className="flex flex-col items-center py-4 text-text-muted border-b border-border-strong flex-shrink-0">
                <p className="text-[13px] font-medium text-text-base mb-1">Stale directory</p>
                <p className="text-[12px]">
                  This skill exists on disk but is not in the project config.
                  {staleSkillName && (
                    <> Choose how to resolve <span className="font-mono font-medium text-text-base">{staleSkillName}</span>.</>
                  )}
                </p>
              </div>
              <div className="flex items-center gap-2 px-5 py-1.5 border-b border-border-strong bg-bg-input flex-shrink-0">
                <span className="text-[11px] text-text-muted">Content on disk (SKILL.md)</span>
              </div>
              <table className="w-full border-collapse">
                <tbody>
                  {file.actual.split("\n").map((line, idx) => (
                    <tr key={idx} className="hover:bg-surface-hover">
                      <td className="select-none text-right text-border-strong px-3 py-0.5 w-12 border-r border-border-strong min-w-[3rem]">
                        {idx + 1}
                      </td>
                      <td className="px-3 py-0.5 whitespace-pre text-text-muted">
                        {line}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
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
                  {staleSkillName && (
                    <p className="text-[12px] mt-3 text-text-base">
                      Choose how to resolve <span className="font-mono font-medium">{staleSkillName}</span>:
                    </p>
                  )}
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
        <div className="flex items-center justify-between px-5 py-3 border-t border-border-strong flex-shrink-0">
          {/* Stale skill resolution actions */}
          {file.reason === "stale" && staleSkillName && projectName ? (
            <>
              <div className="flex items-center gap-2">
                <button
                  onClick={handleAdoptSkill}
                  disabled={actionInProgress !== null}
                  className="px-3 py-1.5 text-[12px] font-medium rounded bg-success/15 text-success border border-success/30 hover:bg-success/25 hover:border-success/50 transition-colors disabled:opacity-50"
                >
                  {actionInProgress === "adopt" ? "Adding..." : "Add to project"}
                </button>
                <button
                  onClick={handleSyncOverwrite}
                  disabled={actionInProgress !== null}
                  className="px-3 py-1.5 text-[12px] font-medium rounded bg-warning/15 text-warning border border-warning/30 hover:bg-warning/25 hover:border-warning/50 transition-colors disabled:opacity-50"
                >
                  {actionInProgress === "overwrite" ? "Syncing..." : "Overwrite (re-sync)"}
                </button>
                <button
                  onClick={handleRemoveSkill}
                  disabled={actionInProgress !== null}
                  className="px-3 py-1.5 text-[12px] font-medium rounded bg-danger/15 text-danger border border-danger/30 hover:bg-danger/25 hover:border-danger/50 transition-colors disabled:opacity-50"
                >
                  {actionInProgress === "remove" ? "Removing..." : "Remove from disk"}
                </button>
              </div>
              <button
                onClick={onClose}
                className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
              >
                Close
              </button>
            </>
          ) : (
            <div className="ml-auto">
              <button
                onClick={onClose}
                className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
              >
                Close
              </button>
            </div>
          )}
        </div>
      </div>

    </div>
  );
}

// ── InstructionConflictModal ──────────────────────────────────────────────────

interface InstructionConflictModalProps {
  conflict: InstructionFileConflict;
  projectName: string;
  onAdopt: (adoptedContent: string) => void;
  onOverwrite: () => void;
  onClose: () => void;
}

function InstructionConflictModal({
  conflict,
  projectName: _projectName,
  onAdopt,
  onOverwrite,
  onClose,
}: InstructionConflictModalProps) {
  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const hasAutomaticContent = conflict.automatic_content.trim().length > 0;

  // Compute a simple line-level diff (LCS-based)
  type DiffLine = { type: "same" | "added" | "removed"; text: string };
  const diffLines = useMemo((): DiffLine[] => {
    const aLines = (hasAutomaticContent ? conflict.automatic_content : "").split("\n");
    const bLines = conflict.disk_content.split("\n");
    // LCS table
    const m = aLines.length, n = bLines.length;
    const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
    for (let i = m - 1; i >= 0; i--) {
      for (let j = n - 1; j >= 0; j--) {
        dp[i]![j] = aLines[i] === bLines[j]
          ? 1 + (dp[i + 1]?.[j + 1] ?? 0)
          : Math.max(dp[i + 1]?.[j] ?? 0, dp[i]?.[j + 1] ?? 0);
      }
    }
    // Traceback
    const result: DiffLine[] = [];
    let i = 0, j = 0;
    while (i < m || j < n) {
      if (i < m && j < n && aLines[i] === bLines[j]) {
        result.push({ type: "same", text: aLines[i]! });
        i++; j++;
      } else if (j < n && (i >= m || (dp[i + 1]?.[j] ?? 0) <= (dp[i]?.[j + 1] ?? 0))) {
        result.push({ type: "added", text: bLines[j]! });
        j++;
      } else {
        result.push({ type: "removed", text: aLines[i]! });
        i++;
      }
    }
    return result;
  }, [conflict.automatic_content, conflict.disk_content, hasAutomaticContent]);

  const addedCount = diffLines.filter((l: DiffLine) => l.type === "added").length;
  const removedCount = diffLines.filter((l: DiffLine) => l.type === "removed").length;
  const diskLineCount = conflict.disk_content.split("\n").length;
  const noDiff = addedCount === 0 && removedCount === 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border-strong/40 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: "min(640px, 90vw)", maxHeight: "85vh" }}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-border-strong flex-shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-[11px] font-medium text-warning/70 uppercase tracking-wider">
              Instruction File Conflict
            </span>
            <span className="text-border-strong">/</span>
            <span className="text-[13px] font-mono text-text-base">{conflict.filename}</span>
          </div>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text-base transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className="px-5 py-4 space-y-4 overflow-y-auto flex-1 min-h-0">
          <p className="text-[13px] text-text-base">
            <span className="font-mono text-warning">{conflict.filename}</span>
            {" "}has been modified outside Automatic.
          </p>

          {/* Diff view */}
          <div className="rounded-lg border border-border-strong/40 overflow-hidden">
            <div className="bg-bg-input px-3 py-2 flex items-center justify-between border-b border-border-strong/30">
              <span className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
                {hasAutomaticContent ? "Changes vs Automatic" : "On disk"}
              </span>
              <span className="text-[11px] text-text-muted flex items-center gap-2">
                {hasAutomaticContent && !noDiff && (
                  <>
                    {addedCount > 0 && <span className="text-success">+{addedCount}</span>}
                    {removedCount > 0 && <span className="text-danger">−{removedCount}</span>}
                    <span className="text-border-strong/60">·</span>
                  </>
                )}
                <span>{diskLineCount} line{diskLineCount !== 1 ? "s" : ""}</span>
              </span>
            </div>
            <pre className="text-[12px] font-mono p-0 whitespace-pre-wrap leading-relaxed max-h-64 overflow-y-auto">
              {noDiff || !hasAutomaticContent ? (
                <span className="block p-3 text-text-muted">
                  {conflict.disk_content.trim() || <em className="not-italic text-text-subtle">empty</em>}
                </span>
              ) : (
                diffLines.map((line: DiffLine, idx: number) => (
                  <span
                    key={idx}
                    className={
                      line.type === "added"
                        ? "block px-3 py-px bg-success/10 text-success"
                        : line.type === "removed"
                        ? "block px-3 py-px bg-danger/10 text-danger line-through decoration-danger/40"
                        : "block px-3 py-px text-text-muted"
                    }
                  >
                    <span className="select-none mr-2 opacity-50 w-4 inline-block text-right">
                      {line.type === "added" ? "+" : line.type === "removed" ? "−" : " "}
                    </span>
                    {line.text || " "}
                  </span>
                ))
              )}
            </pre>
          </div>

          <div className="grid grid-cols-1 gap-2">
            <button
              onClick={() => onAdopt(conflict.disk_content)}
              className="flex flex-col items-start gap-0.5 px-4 py-3 rounded-lg border border-success/30 bg-success/5 hover:bg-success/10 hover:border-success/50 transition-colors text-left"
            >
              <span className="text-[13px] font-medium text-success">Use existing file</span>
              <span className="text-[12px] text-text-muted">
                Keep the on-disk content and load it into Automatic's editor.
              </span>
            </button>

            <button
              onClick={onOverwrite}
              className="flex flex-col items-start gap-0.5 px-4 py-3 rounded-lg border border-danger/30 bg-danger/5 hover:bg-danger/10 hover:border-danger/50 transition-colors text-left"
            >
              <span className="text-[13px] font-medium text-danger">Overwrite with Automatic content</span>
              <span className="text-[12px] text-text-muted">
                {hasAutomaticContent
                  ? "Replace the on-disk file with Automatic's editor content."
                  : "Discard external changes. Only configured rules will remain."}
              </span>
            </button>
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong flex-shrink-0">
          <button
            onClick={onClose}
            className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
          >
            Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}

interface RebuildConfirmationModalProps {
  preview: RebuildPreview;
  busy: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

function RebuildConfirmationModal({ preview, busy, onConfirm, onClose }: RebuildConfirmationModalProps) {
  const categories = Array.isArray(preview.categories) ? preview.categories : [];

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [busy, onClose]);

  const changedCategories = categories.filter((category) => category.added.length > 0 || category.removed.length > 0);
  const unchangedCategories = categories.filter((category) => category.added.length === 0 && category.removed.length === 0);

  const renderPills = (items: string[], tone: "neutral" | "success" | "danger") => {
    if (items.length === 0) {
      return <span className="text-[11px] italic text-text-muted/70">none</span>;
    }

    const toneClass = tone === "success"
      ? "bg-success/10 text-success border-success/20"
      : tone === "danger"
      ? "bg-danger/10 text-danger border-danger/20"
      : "bg-bg-input text-text-base border-border-strong/40";

    return (
      <div className="flex flex-wrap gap-1.5">
        {items.map((item) => (
          <span key={item} className={`inline-flex items-center px-2 py-0.5 rounded-full border text-[11px] font-medium ${toneClass}`}>
            {item}
          </span>
        ))}
      </div>
    );
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => { if (e.target === e.currentTarget && !busy) onClose(); }}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border-strong/40 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: "min(860px, 92vw)", maxHeight: "85vh" }}
      >
        <div className="flex items-center justify-between px-5 py-3 border-b border-border-strong flex-shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-[11px] font-medium text-warning/70 uppercase tracking-wider">Rebuild Project</span>
            <span className="text-border-strong">/</span>
            <span className="text-[13px] font-mono text-text-base">{preview.project_name}</span>
          </div>
          <button onClick={onClose} disabled={busy} className="text-text-muted hover:text-text-base transition-colors disabled:opacity-40">
            <X size={16} />
          </button>
        </div>

        <div className="px-5 py-4 space-y-4 overflow-y-auto flex-1 min-h-0">
          <div className="rounded-lg border border-warning/25 bg-warning/5 px-4 py-3">
            <p className="text-[13px] text-text-base leading-relaxed">
              Rebuild will replace Automatic's saved project state with what it can detect from the current project files on disk.
            </p>
            <p className="text-[12px] text-text-muted mt-1">
              This compares feature state, not file contents.
            </p>
          </div>

          {changedCategories.length > 0 ? (
            <div className="space-y-3">
              {changedCategories.map((category) => (
                <div key={category.key} className="rounded-lg border border-border-strong/40 overflow-hidden">
                  <div className="px-3 py-2 bg-bg-input border-b border-border-strong/30 flex items-center justify-between gap-3">
                    <span className="text-[12px] font-semibold text-text-base">{category.label}</span>
                    <div className="flex items-center gap-2 text-[11px] font-medium">
                      {category.added.length > 0 && <span className="text-success">+{category.added.length}</span>}
                      {category.removed.length > 0 && <span className="text-danger">-{category.removed.length}</span>}
                    </div>
                  </div>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-px bg-border-strong/20">
                    <div className="bg-bg-sidebar p-3 space-y-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted">Automatic now</div>
                      {renderPills(category.automatic, "neutral")}
                    </div>
                    <div className="bg-bg-sidebar p-3 space-y-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted">Detected on disk</div>
                      {renderPills(category.disk, "neutral")}
                    </div>
                  </div>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-px bg-border-strong/20 border-t border-border-strong/30">
                    <div className="bg-bg-sidebar p-3 space-y-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wider text-success">Will be added</div>
                      {renderPills(category.added, "success")}
                    </div>
                    <div className="bg-bg-sidebar p-3 space-y-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wider text-danger">Will be removed</div>
                      {renderPills(category.removed, "danger")}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3 text-[13px] text-text-muted">
              No feature differences detected. Rebuild will just refresh Automatic's stored snapshots and derived state.
            </div>
          )}

          {unchangedCategories.length > 0 && (
            <div className="rounded-lg border border-border-strong/30 bg-bg-input/40 px-4 py-3">
              <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted mb-2">Unchanged</div>
              <div className="flex flex-wrap gap-1.5">
                {unchangedCategories.map((category) => (
                  <span key={category.key} className="inline-flex items-center px-2 py-0.5 rounded-full border border-border-strong/30 text-[11px] text-text-muted">
                    {category.label}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong flex-shrink-0 bg-bg-input">
          <button
            onClick={onClose}
            disabled={busy}
            className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors disabled:opacity-40"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={busy}
            className="px-3 py-1.5 text-[12px] font-medium rounded bg-warning/15 text-warning border border-warning/30 hover:bg-warning/25 hover:border-warning/50 transition-colors disabled:opacity-50"
          >
            {busy ? "Rebuilding..." : "Confirm Rebuild"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Projects Overview (card grid) ────────────────────────────────────────────

interface ProjectsOverviewProps {
  projects: string[];
  projectsLoading: boolean;
  projectDetails: Map<string, Project>;
  driftByProject: Record<string, boolean>;
  folders: ProjectFolder[];
  onSelect: (name: string) => void;
  onCreate: () => void;
  onSyncAll?: () => void;
  syncAllStatus?: "idle" | "syncing";
  /** When set, the overview scopes its card grid and health bar to this folder only. */
  selectedFolder?: ProjectFolder | null;
  /** Called when the user dismisses the active folder filter. */
  onClearFolder?: () => void;
}

function ProjectStatusBadge({ drift }: { drift: boolean | undefined }) {
  if (drift === true) {
    return (
      <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-semibold bg-warning/10 text-warning border border-warning/20">
        <AlertCircle size={8} />
        Drifted
      </span>
    );
  }
  if (drift === false) {
    return (
      <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-semibold bg-success/10 text-success border border-success/20">
        <Check size={8} />
        Synced
      </span>
    );
  }
  // Reserve the same vertical space as the badge so cards don't shift height
  // once drift is determined. The span is invisible but occupies the same
  // line-height as the real badge (py-0.5 + text-[10px]).
  return <span className="inline-flex items-center px-1.5 py-0.5 text-[10px] border border-transparent invisible">–</span>;
}

function ProjectCard({
  name,
  project,
  drift,
  onSelect,
}: {
  name: string;
  project: Project | undefined;
  drift: boolean | undefined;
  onSelect: (name: string) => void;
}) {
  const isDrifted = drift === true;
  const isConfigured = !!(project?.directory && (project?.agents?.length ?? 0) > 0);

  const borderClass = isDrifted
    ? "border-warning/30 hover:border-warning/50"
    : "border-border-strong/40 hover:border-border-strong/70";

  const totalSkills = (project?.skills?.length ?? 0) + (project?.local_skills?.length ?? 0);
  const mcpCount = project?.mcp_servers?.length ?? 0;
  const agentCount = project?.agents?.length ?? 0;

  return (
    <button
      onClick={() => onSelect(name)}
      className={`group relative w-full h-full text-left bg-bg-input border ${borderClass} rounded-xl p-3 flex flex-col gap-2.5 transition-all hover:bg-surface-hover hover:-translate-y-0.5`}
    >
      {/* Row 1: title + status */}
      <div className="flex items-start gap-2.5">
        <div
          className={`mt-0.5 flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md border ${
            isDrifted
              ? "border-warning/30 bg-warning/10"
              : "border-border-strong/40 bg-bg-sidebar"
          }`}
        >
          <FolderOpen
            size={12}
            className={`flex-shrink-0 ${isDrifted ? "text-warning" : "text-icon-agent"}`}
          />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-semibold text-text-base truncate">{name}</div>
          <div className="text-[11px] text-text-muted mt-0.5">{agentCount} agent{agentCount !== 1 ? "s" : ""}</div>
        </div>
        <div className="flex-shrink-0 mt-0.5">
          {isConfigured ? (
            <ProjectStatusBadge drift={drift} />
          ) : (
            <span className="inline-flex items-center px-1.5 py-0.5 text-[10px] border border-transparent invisible">-</span>
          )}
        </div>
      </div>

      {/* Row 2: agent chips */}
      {(project?.agents?.length ?? 0) > 0 ? (
        <div className="flex items-center gap-1 flex-wrap">
          {(project?.agents ?? []).map((agentId) => (
            <span
              key={agentId}
              className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted"
            >
              <AgentIcon agentId={agentId} size={9} />
              {agentId}
            </span>
          ))}
        </div>
      ) : (
        <div className="flex items-center gap-1 text-[11px] text-warning/70">
          <AlertCircle size={10} className="flex-shrink-0" />
          <span>No agents configured</span>
        </div>
      )}

      {/* Row 3: stats footer */}
      <div className="mt-auto flex w-full items-center gap-3 pt-2 border-t border-border-strong/30 text-[11px] text-text-muted">
        <span className="flex items-center gap-1">
          <Code size={10} />
          {totalSkills}
        </span>
        <span className="flex items-center gap-1">
          <Server size={10} />
          {mcpCount}
        </span>
        {project?.updated_at && (
          <span className="ml-auto whitespace-nowrap text-text-muted/70">
            {new Date(project.updated_at).toLocaleDateString(undefined, { month: "short", day: "numeric" })}
          </span>
        )}
      </div>
    </button>
  );
}

// ── Projects Health Bar ───────────────────────────────────────────────────────

interface ProjectsHealthBarProps {
  projects: string[];
  projectDetails: Map<string, Project>;
  driftByProject: Record<string, boolean>;
}

function ProjectsHealthBar({ projects, projectDetails, driftByProject }: ProjectsHealthBarProps) {
  if (projects.length === 0) return null;

  const total = projects.length;
  const synced = projects.filter((n) => driftByProject[n] === false).length;
  const drifted = projects.filter((n) => driftByProject[n] === true).length;
  const checking = projects.filter((n) => driftByProject[n] === undefined).length;

  // Unique agent ids across all projects
  const agentSet = new Set<string>();
  let totalSkills = 0;
  let totalMcp = 0;
  let fullyConfigured = 0;
  for (const name of projects) {
    const p = projectDetails.get(name);
    if (!p) continue;
    (p.agents ?? []).forEach((a) => agentSet.add(a));
    totalSkills += (p.skills?.length ?? 0) + (p.local_skills?.length ?? 0);
    totalMcp += p.mcp_servers?.length ?? 0;
    if ((p.agents?.length ?? 0) > 0 && !!p.directory) fullyConfigured++;
  }

  // Show a compact progress-like bar for synced/drifted/checking ratio
  const syncedPct = total > 0 ? Math.round((synced / total) * 100) : 0;
  const driftedPct = total > 0 ? Math.round((drifted / total) * 100) : 0;
  const checkingPct = total > 0 ? Math.max(0, 100 - syncedPct - driftedPct) : 0;

  return (
    <div className="rounded-xl border border-border-strong/40 bg-bg-input overflow-hidden">
      {/* Stat strip */}
      <div className="flex items-stretch divide-x divide-border-strong/30">
        {/* Projects */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div className="flex items-center gap-1 text-text-base">
            <FolderOpen size={13} />
            <span className="text-[15px] font-semibold tabular-nums leading-none">{total}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Projects</span>
        </div>

        {/* Synced — uses health token so corporate themes get luminance-stepped grey */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div
            className="flex items-center gap-1"
            style={{ color: synced > 0 ? "var(--health-synced)" : undefined }}
          >
            <Check size={13} className={synced === 0 ? "text-text-muted" : ""} />
            <span className={`text-[15px] font-semibold tabular-nums leading-none ${synced === 0 ? "text-text-muted" : ""}`}>{synced}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Synced</span>
        </div>

        {/* Drifted — uses health token */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div
            className="flex items-center gap-1"
            style={{ color: drifted > 0 ? "var(--health-drifted)" : undefined }}
          >
            <AlertCircle size={13} className={drifted === 0 ? "text-text-muted" : ""} />
            <span className={`text-[15px] font-semibold tabular-nums leading-none ${drifted === 0 ? "text-text-muted" : ""}`}>{drifted}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Drifted</span>
        </div>

        {/* Agents */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div className={`flex items-center gap-1 ${agentSet.size > 0 ? "text-brand" : "text-text-muted"}`}>
            <Bot size={13} />
            <span className="text-[15px] font-semibold tabular-nums leading-none">{agentSet.size}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Agents</span>
        </div>

        {/* Skills */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div className={`flex items-center gap-1 ${totalSkills > 0 ? "text-icon-skill" : "text-text-muted"}`}>
            <Code size={13} />
            <span className="text-[15px] font-semibold tabular-nums leading-none">{totalSkills}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Skills</span>
        </div>

        {/* MCP Servers */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div className={`flex items-center gap-1 ${totalMcp > 0 ? "text-icon-mcp" : "text-text-muted"}`}>
            <Server size={13} />
            <span className="text-[15px] font-semibold tabular-nums leading-none">{totalMcp}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">MCP Servers</span>
        </div>
      </div>

      {/* Sync health bar — only shown when we have drift data for at least one project */}
      {checking < total && (
        <div className="border-t border-border-strong/30 px-4 py-2 flex items-center gap-3">
          <span className="text-[10px] text-text-muted uppercase tracking-wider flex-shrink-0">Sync health</span>
          <div className="flex-1 h-1.5 rounded-full overflow-hidden flex" style={{ background: "var(--health-checking)" }}>
            {syncedPct > 0 && (
              <div
                className="h-full transition-all"
                style={{ width: `${syncedPct}%`, background: "var(--health-synced)" }}
                title={`${synced} synced`}
              />
            )}
            {driftedPct > 0 && (
              <div
                className="h-full transition-all"
                style={{ width: `${driftedPct}%`, background: "var(--health-drifted)" }}
                title={`${drifted} drifted`}
              />
            )}
            {checkingPct > 0 && (
              <div
                className="h-full transition-all"
                style={{ width: `${checkingPct}%`, background: "var(--health-checking)" }}
                title={`${checking} checking`}
              />
            )}
          </div>
          <div className="flex items-center gap-2.5 flex-shrink-0 text-[10px]">
            {synced > 0 && (
              <span style={{ color: "var(--health-synced)" }}>{syncedPct}% synced</span>
            )}
            {drifted > 0 && (
              <span style={{ color: "var(--health-drifted)" }}>{drifted} drifted</span>
            )}
            {checking > 0 && <span className="text-text-muted">{checking} checking…</span>}
            {fullyConfigured < total && (
              <span className="text-text-muted">{total - fullyConfigured} unconfigured</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

function ProjectsOverview({ projects, projectsLoading, projectDetails, driftByProject, folders, onSelect, onCreate, onSyncAll, syncAllStatus, selectedFolder, onClearFolder }: ProjectsOverviewProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [sortOrder, setSortOrder] = useState<"sidebar" | "alphabetical" | "created" | "updated" | "last_activity">("sidebar");
  // Track which folder groups are collapsed in the overview (independent of sidebar state)
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});

  const getSortTimestamp = (project: Project | undefined, key: "created" | "updated" | "last_activity"): number => {
    if (!project) return 0;
    if (key === "created") return new Date(project.created_at ?? 0).getTime();
    if (key === "updated") return new Date(project.updated_at ?? 0).getTime();
    return new Date(project.last_activity ?? project.updated_at ?? project.created_at ?? 0).getTime();
  };

  const sortNames = (names: string[]) => {
    if (sortOrder === "sidebar") return names;
    return [...names].sort((a, b) => {
      if (sortOrder === "alphabetical") return a.localeCompare(b);
      const aTime = getSortTimestamp(projectDetails.get(a), sortOrder);
      const bTime = getSortTimestamp(projectDetails.get(b), sortOrder);
      return bTime - aTime;
    });
  };

  const matchesSearch = (name: string): boolean => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return true;
    const details = projectDetails.get(name);
    return (
      name.toLowerCase().includes(query) ||
      (details?.directory ?? "").toLowerCase().includes(query) ||
      (details?.agents ?? []).some((agent) => agent.toLowerCase().includes(query))
    );
  };

  // When a folder filter is active, restrict the universe of projects to that
  // folder only. This feeds both the card grid and the health bar.
  const visibleProjects = selectedFolder
    ? selectedFolder.projectNames.filter((n) => projects.includes(n))
    : projects;

  // Build grouped structure. Folders that have at least one visible project are
  // rendered as groups; the remainder form the ungrouped section.
  const projectsInFolders = new Set(folders.flatMap((f) => f.projectNames));
  const ungroupedNames = visibleProjects.filter((n) => !projectsInFolders.has(n));
  // Only show folder grouping in the grid when there is no active folder filter
  const hasFolders = !selectedFolder && folders.some((f) => f.projectNames.some((n) => projects.includes(n)));

  // Filtered + sorted per section
  const filteredFolders = folders
    .map((folder) => ({
      folder,
      visibleNames: sortNames(
        folder.projectNames.filter((n) => projects.includes(n) && matchesSearch(n))
      ),
    }))
    .filter(({ visibleNames }) => visibleNames.length > 0);

  const filteredUngrouped = selectedFolder
    ? sortNames(visibleProjects.filter(matchesSearch))
    : sortNames(ungroupedNames.filter(matchesSearch));

  const totalVisible =
    filteredFolders.reduce((s, { visibleNames }) => s + visibleNames.length, 0) +
    filteredUngrouped.length;

  const toggleGroup = (id: string) =>
    setCollapsedGroups((prev) => ({ ...prev, [id]: !prev[id] }));

  const renderCardGrid = (names: string[]) => (
    <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3">
      {names.map((name) => (
        <ProjectCard
          key={name}
          name={name}
          project={projectDetails.get(name)}
          drift={driftByProject[name]}
          onSelect={onSelect}
        />
      ))}
    </div>
  );

  return (
    <div className="flex-1 h-full overflow-y-auto custom-scrollbar bg-bg-base">
      {/* Top bar */}
      <div className="h-11 px-6 border-b border-border-strong/40 flex items-center justify-between bg-bg-base/50 flex-shrink-0">
        {/* Title / active folder breadcrumb */}
        {selectedFolder ? (
          <div className="flex items-center gap-1.5 min-w-0">
            <button
              onClick={onClearFolder}
              className="text-[11px] font-semibold text-text-muted tracking-wider uppercase hover:text-text-base transition-colors"
            >
              Projects
            </button>
            <ChevronRight size={11} className="text-text-muted flex-shrink-0" />
            <span className="flex items-center gap-1 text-[11px] font-semibold text-text-base tracking-wider uppercase truncate">
              <Folder size={11} className="flex-shrink-0" />
              {selectedFolder.name}
            </span>
            <button
              onClick={onClearFolder}
              className="ml-1 p-0.5 rounded text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors flex-shrink-0"
              title="Clear folder filter"
            >
              <X size={11} />
            </button>
          </div>
        ) : (
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Projects
          </span>
        )}
        <div className="flex items-center gap-2">
          <div className="relative">
            <Search size={12} className="absolute left-2 top-1/2 -translate-y-1/2 text-text-muted" />
            <input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search projects"
              className="h-7 w-44 rounded-md border border-border-strong/50 bg-bg-input pl-7 pr-2 text-[12px] text-text-base placeholder:text-text-muted focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60"
            />
          </div>
          <div className="relative">
            <select
              value={sortOrder}
              onChange={(e) => setSortOrder(e.target.value as "sidebar" | "alphabetical" | "created" | "updated" | "last_activity")}
              className="h-7 min-w-[120px] appearance-none rounded-md border border-border-strong/50 bg-bg-input px-2.5 pr-7 text-[12px] text-text-base shadow-none focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60"
              aria-label="Sort projects"
            >
              <option value="sidebar">Sidebar order</option>
              <option value="alphabetical">Alphabetical</option>
              <option value="created">Created</option>
              <option value="updated">Updated</option>
              <option value="last_activity">Last Activity</option>
            </select>
            <ChevronDown
              size={12}
              className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted"
            />
          </div>
          {onSyncAll && visibleProjects.some((n) => driftByProject[n] === true) && (
            <button
              onClick={onSyncAll}
              disabled={syncAllStatus === "syncing"}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-warning/10 hover:bg-warning/20 text-warning border border-warning/30 rounded text-[12px] font-medium transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
            >
              <RefreshCw size={12} className={syncAllStatus === "syncing" ? "animate-spin" : ""} />
              {syncAllStatus === "syncing" ? "Syncing…" : "Sync all"}
            </button>
          )}
          <button
            onClick={onCreate}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors shadow-sm"
          >
            <Plus size={12} /> New Project
          </button>
        </div>
      </div>

      <div className="p-6 space-y-6">
        {/* Health overview bar — scoped to visibleProjects when a folder is active */}
        {!projectsLoading && visibleProjects.length > 0 && (
          <ProjectsHealthBar
            projects={visibleProjects}
            projectDetails={projectDetails}
            driftByProject={driftByProject}
          />
        )}

        {/* Empty state */}
        {!projectsLoading && projects.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <div className="w-16 h-16 rounded-2xl border border-dashed border-border-strong flex items-center justify-center mb-5">
              <FolderOpen size={24} className="text-text-muted" />
            </div>
            <h2 className="text-[16px] font-semibold text-text-base mb-2">No projects yet</h2>
            <p className="text-[13px] text-text-muted mb-6 leading-relaxed max-w-xs">
              Projects group your agent configurations, skills, and MCP servers for a specific codebase.
            </p>
            <button
              onClick={onCreate}
              className="px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              Create Project
            </button>
          </div>
        ) : totalVisible === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center border border-border-strong/30 rounded-lg bg-bg-input/40">
            <p className="text-[13px] text-text-base mb-1">No matching projects</p>
            <p className="text-[12px] text-text-muted">Try another search term.</p>
          </div>
        ) : hasFolders ? (
          /* ── Grouped layout ── */
          <div className="space-y-6">
            {filteredFolders.map(({ folder, visibleNames }) => {
              const isCollapsed = collapsedGroups[folder.id] ?? false;
              return (
                <div key={folder.id}>
                  {/* Group header */}
                  <button
                    onClick={() => toggleGroup(folder.id)}
                    className="flex items-center gap-2 mb-3 group/gh w-full text-left"
                  >
                    <ChevronDown
                      size={13}
                      className={`text-text-muted transition-transform flex-shrink-0 ${isCollapsed ? "-rotate-90" : ""}`}
                    />
                    <Folder size={13} className="text-text-muted flex-shrink-0" />
                    <span className="text-[12px] font-semibold text-text-muted tracking-wide">
                      {folder.name}
                    </span>
                    <span className="text-[11px] text-text-muted/60 ml-0.5">
                      {visibleNames.length}
                    </span>
                  </button>
                  {!isCollapsed && renderCardGrid(visibleNames)}
                </div>
              );
            })}

            {/* Ungrouped projects at the bottom */}
            {filteredUngrouped.length > 0 && (
              <div>
                <div className="flex items-center gap-2 mb-3">
                  <span className="text-[12px] font-semibold text-text-muted/60 tracking-wide">Other</span>
                  <span className="text-[11px] text-text-muted/40">{filteredUngrouped.length}</span>
                </div>
                {renderCardGrid(filteredUngrouped)}
              </div>
            )}
          </div>
        ) : (
          /* ── Flat layout (no folders / folder filter active) ── */
          renderCardGrid(filteredUngrouped)
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

/**
 * Installs an AI-suggested skill from the remote registry and then notifies
 * the parent to add it to the project config.
 *
 * Error handling is explicit: on failure the button shows an error message and
 * nothing is added to the project — no broken references are created.
 */
function SkillAddButton({
  rec,
  alreadyAdded,
  onAdd,
}: {
  rec: ProjectRecommendation;
  alreadyAdded: boolean;
  onAdd: (skillName: string) => Promise<boolean> | boolean;
}) {
  const [state, setState] = useState<"idle" | "loading" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const handleAdd = async () => {
    setState("loading");
    setErrorMsg(null);

    // 1. Resolve skill metadata from the stored blob or by searching.
    let meta: { id: string; name: string; source: string } | null = null;
    if (rec.metadata) {
      try {
        const parsed = JSON.parse(rec.metadata) as { id: string; name: string; source: string };
        if (parsed.name && parsed.source) meta = parsed;
      } catch { /* fall through to search */ }
    }
    if (!meta) {
      try {
        const results = await invoke<{ id: string; name: string; source: string; installs: number }[]>(
          "search_remote_skills", { query: rec.title },
        );
        const match = results.find((r) => r.name === rec.title) ?? results[0];
        if (match) meta = { id: match.id, name: match.name, source: match.source };
      } catch { /* search failed */ }
    }

    if (!meta) {
      setState("error");
      setErrorMsg("Could not find this skill in the registry.");
      return;
    }

    // 2. Fetch the skill content from the remote registry.
    let content: string;
    try {
      content = await invoke("fetch_remote_skill_content", { source: meta.source, name: meta.name });
    } catch (err: any) {
      setState("error");
      setErrorMsg(`Failed to fetch skill: ${err}`);
      return;
    }

    // 3. Install it locally.
    try {
      await invoke("import_remote_skill", { name: meta.name, content, source: meta.source, id: meta.id });
    } catch (err: any) {
      setState("error");
      setErrorMsg(`Failed to install skill: ${err}`);
      return;
    }

    // 4. Everything succeeded — add to project and dismiss the card.
    setState("idle");
    const added = await Promise.resolve(onAdd(meta.name));
    if (!added) {
      setState("error");
      setErrorMsg("Installed skill, but failed to add it to this project.");
      return;
    }
  };

  if (errorMsg) {
    return (
      <span className="text-[11px] text-error flex items-center gap-1">
        <AlertCircle size={10} /> {errorMsg}
      </span>
    );
  }

  return (
    <button
      onClick={handleAdd}
      disabled={alreadyAdded || state === "loading"}
      className="text-[11px] font-medium text-brand hover:text-brand-hover border border-brand/40 rounded px-2 py-1 transition-colors flex items-center gap-1 disabled:opacity-40 disabled:cursor-default"
    >
      {state === "loading"
        ? <><RefreshCw size={10} className="animate-spin" /> Installing…</>
        : <><Plus size={10} /> {alreadyAdded ? "Added" : "Add to project"}</>
      }
    </button>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

/**
 * Installs an AI-suggested MCP server config and adds it to the project.
 *
 * Looks up the server in the featured marketplace data by title/slug match,
 * builds its config JSON, saves it via `save_mcp_server_config`, then notifies
 * the parent to add it to the project config.
 *
 * On failure the button shows an error — nothing is added to the project.
 */
function McpAddButton({
  rec,
  alreadyAdded,
  onAdd,
}: {
  rec: ProjectRecommendation;
  alreadyAdded: boolean;
  onAdd: (serverName: string) => Promise<boolean> | boolean;
}) {
  const [state, setState] = useState<"idle" | "loading" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const handleAdd = async () => {
    setState("loading");
    setErrorMsg(null);

    // Find the server in the local marketplace catalogue by slug or title.
    const needle = rec.title.toLowerCase();
    const servers = mcpServersData as Array<{
      slug: string; name: string; title: string; provider: string;
      repository_url: string | null;
      remote: { transport: string; url: string } | null;
      local: { registry: string; package: string; version: string | null; transport: string; command: string } | null;
      auth: { method: string; env_vars: Array<{ name: string; description: string; secret: boolean }> };
    }>;
    const server = servers.find(
      (s) => s.slug === needle || s.title.toLowerCase() === needle || s.name.toLowerCase() === needle,
    );

    if (!server) {
      setState("error");
      setErrorMsg("Server not found in the marketplace catalogue. Use the MCP Marketplace to add it manually.");
      return;
    }

    // Build the config — same logic as McpMarketplace.buildConfig.
    const _author: Record<string, string> = { name: server.provider };
    if (server.repository_url) _author.repository_url = server.repository_url;

    let config: Record<string, unknown>;
    if (server.local) {
      const parts = server.local.command.split(/\s+/);
      const cmd = parts[0] ?? "";
      const args = parts.slice(1);
      const env: Record<string, string> = {};
      server.auth.env_vars.forEach((v) => { env[v.name] = ""; });
      config = { type: "stdio", command: cmd, _author };
      if (args.length > 0) config.args = args;
      if (Object.keys(env).length > 0) config.env = env;
    } else if (server.remote) {
      const type = server.remote.transport === "sse" ? "sse" : "http";
      config = { type, url: server.remote.url, _author };
    } else {
      config = { type: "stdio", command: "", _author };
    }

    // Derive the config key — same as McpMarketplace.configName.
    const configKey = server.title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

    try {
      await invoke("save_mcp_server_config", { name: configKey, data: JSON.stringify(config) });
    } catch (err: any) {
      setState("error");
      setErrorMsg(`Failed to save server config: ${err}`);
      return;
    }

    setState("idle");
    const added = await Promise.resolve(onAdd(configKey));
    if (!added) {
      setState("error");
      setErrorMsg("Server was saved, but failed to add it to this project.");
      return;
    }
  };

  if (errorMsg) {
    return (
      <span className="text-[11px] text-error flex items-center gap-1">
        <AlertCircle size={10} /> {errorMsg}
      </span>
    );
  }

  return (
    <button
      onClick={handleAdd}
      disabled={alreadyAdded || state === "loading"}
      className="text-[11px] font-medium text-brand hover:text-brand-hover border border-brand/40 rounded px-2 py-1 transition-colors flex items-center gap-1 disabled:opacity-40 disabled:cursor-default"
    >
      {state === "loading"
        ? <><RefreshCw size={10} className="animate-spin" /> Adding…</>
        : <><Plus size={10} /> {alreadyAdded ? "Added" : "Add to project"}</>
      }
    </button>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

function isHttpDocPath(path: string): boolean {
  return path.startsWith("http://") || path.startsWith("https://");
}

function isManagedDocNotePath(path: string): boolean {
  return path.startsWith(".automatic/docs/");
}

function getProjectRelativeDocPath(projectDirectory: string | undefined, path: string): string | null {
  if (!projectDirectory) return null;

  const normalizedDirectory = projectDirectory.replace(/\/+$/, "");
  const normalizedPath = path.replace(/\/+$/, "");

  if (normalizedPath === normalizedDirectory) {
    return ".";
  }

  const prefix = `${normalizedDirectory}/`;
  if (!normalizedPath.startsWith(prefix)) {
    return null;
  }

  return normalizedPath.slice(prefix.length);
}

export default function Projects({ resetKey, initialProject = null, onInitialProjectConsumed, initialProjectTab = null, onInitialProjectTabConsumed, onNavigateToSkill, onNavigateToMcpServer, onNavigateToSkillStore, onNavigateToSkillStoreWithResult, onNavigateToMcpMarketplace, onNavigateToGroup, initialCreateWithTemplate = null, onInitialCreateWithTemplateConsumed }: ProjectsProps = {}) {
  const { userId } = useCurrentUser();
  const { log, update } = useTaskLog();
  const LAST_PROJECT_KEY = "automatic.projects.selected";
  const PROJECT_ORDER_KEY = "automatic.projects.order";
  const PROJECT_FOLDERS_KEY = "automatic.projects.folders";

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
  const [projectsLoading, setProjectsLoading] = useState(true);
  // Always start on the overview — do not restore a previously selected project.
  const [selectedName, setSelectedName] = useState<string | null>(null);

  // ── Folder state ────────────────────────────────────────────────────────────
  const [folders, setFolders] = useState<ProjectFolder[]>(() => {
    try {
      const stored = localStorage.getItem("automatic.projects.folders");
      if (!stored) return [];
      return JSON.parse(stored) as ProjectFolder[];
    } catch {
      return [];
    }
  });
  const [editingFolderId, setEditingFolderId] = useState<string | null>(null);
  const [editingFolderName, setEditingFolderName] = useState("");

  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT);
  const isSidebarDragging = useRef(false);

  // ── Folder context filter (overview page only) ─────────────────────────────
  // When a folder header is clicked in the sidebar while on the overview, the
  // card grid and health bar scope down to only that folder's projects.
  // null = show all projects (no folder filter active).
  const [selectedFolderId, setSelectedFolderId] = useState<string | null>(null);

  // ── Sidebar pin / open state ────────────────────────────────────────────────
  const SIDEBAR_PINNED_KEY = "automatic.projects.sidebar.pinned";
  const [sidebarPinned, setSidebarPinned] = useState<boolean>(() => {
    try { return localStorage.getItem(SIDEBAR_PINNED_KEY) === "true"; } catch { return false; }
  });
  const [sidebarExpanded, setSidebarExpanded] = useState(false);
  const sidebarPanelRef = useRef<HTMLDivElement>(null);
  const sidebarTriggerRef = useRef<HTMLDivElement>(null);
  const sidebarOpen = sidebarPinned || sidebarExpanded;

  const openSidebar = useCallback(() => {
    setSidebarExpanded(true);
  }, []);

  const closeSidebar = useCallback(() => {
    setSidebarExpanded(false);
  }, []);

  const toggleSidebarOpen = useCallback(() => {
    setSidebarExpanded((prev) => !prev);
  }, []);

  const toggleSidebarPin = useCallback(() => {
    setSidebarPinned((prev) => {
      const next = !prev;
      try { localStorage.setItem(SIDEBAR_PINNED_KEY, String(next)); } catch {}
      return next;
    });
    setSidebarExpanded(true);
  }, []);

  useEffect(() => {
    if (!sidebarOpen || sidebarPinned) return;

    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (sidebarPanelRef.current?.contains(target) || sidebarTriggerRef.current?.contains(target)) {
        return;
      }
      setSidebarExpanded(false);
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setSidebarExpanded(false);
      }
    };

    document.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [sidebarOpen, sidebarPinned]);

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
  // Ghost label that follows the cursor while dragging
  const [dragGhost, setDragGhost] = useState<{ name: string; x: number; y: number } | null>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  // Wizard state (used while isCreating === true)
  const [wizardStep, setWizardStep] = useState<1 | 2 | 3>(1);
  const [wizardDiscovering, setWizardDiscovering] = useState(false);
  const [wizardDiscoveredAgents, setWizardDiscoveredAgents] = useState<string[]>([]);
  /** Non-empty when the wizard was launched from a "New project from template" action. */
  const [wizardSourceTemplates, setWizardSourceTemplates] = useState<string[]>([]);
  /** Tracks the name of the stub project saved during step 1 so it can be deleted on cancel. */
  const wizardStubName = useRef<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");

  // Available items to pick from
  const [availableAgents, setAvailableAgents] = useState<AgentInfo[]>([]);
  const [availableSkills, setAvailableSkills] = useState<string[]>([]);
  const [availableMcpServers, setAvailableMcpServers] = useState<string[]>([]);

  // Inline add state
  const [syncStatus, setSyncStatus] = useState<string | null>(null);
  // "Sync all" button status for the overview grid
  const [syncAllStatus, setSyncAllStatus] = useState<"idle" | "syncing">("idle");

  // Drift detection state
  // null = unknown/not yet checked, DriftReport = result of last check
  const [driftReport, setDriftReport] = useState<DriftReport | null>(null);
  const driftCheckInFlight = useRef(false);

  // Per-project drift indicator: true = drifted, false = clean, undefined = unknown
  const [driftByProject, setDriftByProject] = useState<Record<string, boolean>>({});

  // Lightweight project details cache for the overview grid (name → Project)
  const [projectDetailsMap, setProjectDetailsMap] = useState<Map<string, Project>>(new Map());

  // Drift diff modal state — null when closed
  const [driftDiffFile, setDriftDiffFile] = useState<{ file: DriftedFile; agentLabel: string } | null>(null);

  // Instruction file conflict modal state — null when closed, conflict when open
  const [instructionConflict, setInstructionConflict] = useState<InstructionFileConflict | null>(null);
  const [rebuildPreview, setRebuildPreview] = useState<RebuildPreview | null>(null);
  const [rebuildBusy, setRebuildBusy] = useState(false);

  // Project template state
  const [availableProjectTemplates, setAvailableProjectTemplates] = useState<ProjectTemplate[]>([]);
  const [showProjectTemplatePicker, setShowProjectTemplatePicker] = useState(false);
  /** Names of templates currently selected in the picker (multi-select). */
  const [selectedProjectTemplates, setSelectedProjectTemplates] = useState<string[]>([]);
  // Pending unified instruction content + rules to write after next save (from template applies).
  // Each entry corresponds to one applied template; contents are concatenated on flush.
  const pendingUnifiedInstruction = useRef<{ content: string; rules: string[] }[] | null>(null);

  // Project file state
  const [projectFiles, setProjectFiles] = useState<ProjectFileInfo[]>([]);
  const [activeProjectFile, setActiveProjectFile] = useState<string | null>(null);
  const [projectFileContent, setProjectFileContent] = useState("");
  const [projectFileEditing, setProjectFileEditing] = useState(false);
  const [projectFileDirty, setProjectFileDirty] = useState(false);
  const [projectFileSaving, setProjectFileSaving] = useState(false);
  const [projectFileGenerating, setProjectFileGenerating] = useState(false);
  const [projectFileUpdating, setProjectFileUpdating] = useState(false);
  // Whether an Anthropic API key is resolvable (env var or keychain).
  // Controls whether AI Generate buttons are enabled.
  const [hasAnthropicKey, setHasAnthropicKey] = useState(false);
  // Incremented whenever any project configuration is mutated (saved, synced,
  // instruction files written, etc.).  A useEffect watches this counter and
  // re-evaluates recommendations after every change.
  const [projectVersion, setProjectVersion] = useState(0);
  const notifyProjectUpdated = () => setProjectVersion((v) => v + 1);
  const [availableTemplates, setAvailableTemplates] = useState<string[]>([]);
  const [showTemplatePicker, setShowTemplatePicker] = useState(false);
  const [availableRules, setAvailableRules] = useState<{ id: string; name: string }[]>([]);

  // Tab navigation within a project
  type ProjectTab = "summary" | "agents" | "commands" | "custom_agents" | "skills" | "mcp_servers" | "groups" | "project_file" | "rules" | "context" | "docs_files" | "docs_links" | "docs_notes" | "memory" | "features" | "activity" | "recommendations";
  type ProjectGroup = "summary" | "configuration" | "instructions" | "documentation" | "runtime" | "planning" | "insights" | "tools";

  const PROJECT_GROUPS: {
    id: ProjectGroup;
    label: string;
    tabs: { id: ProjectTab; label: string }[];
  }[] = [
    { id: "summary", label: "Summary", tabs: [] },
    {
      id: "configuration",
      label: "Configuration",
      tabs: [
        { id: "agents", label: "Providers" },
        { id: "commands", label: "Commands" },
        { id: "custom_agents", label: "Agents" },
        { id: "skills", label: "Skills" },
        { id: "mcp_servers", label: "MCP Servers" },
      ],
    },
    {
      id: "instructions",
      label: "Context",
      tabs: [
        { id: "project_file", label: "Project Instructions" },
        { id: "rules", label: "Rules" },
        { id: "context", label: "Context" },
        { id: "groups", label: "Groups" },
      ],
    },
    {
      id: "documentation",
      label: "Documentation",
      tabs: [
        { id: "docs_files", label: "Files & Dirs" },
        { id: "docs_links", label: "Links" },
        { id: "docs_notes", label: "Notes" },
      ],
    },
    {
      id: "runtime",
      label: "Runtime",
      tabs: [
        { id: "memory", label: "Memory" },
        { id: "activity", label: "Activity" },
      ],
    },
    { id: "planning", label: "Build", tabs: [{ id: "features", label: "Features" }] },
    { id: "insights", label: "Insights", tabs: [{ id: "recommendations", label: "Recommendations" }] },
    // "tools" group has no static tabs — sub-tabs are built dynamically from
    // registered tool entries and rendered separately in the secondary tab bar.
    { id: "tools", label: "Tools", tabs: [] },
  ];

  /** Derive the group for a given tab id */
  function groupForTab(tab: ProjectTab): ProjectGroup {
    for (const g of PROJECT_GROUPS) {
      if (g.id === "summary" && tab === "summary") return "summary";
      if (g.tabs.some((t) => t.id === tab)) return g.id;
    }
    return "summary";
  }

  const [projectTab, setProjectTab] = useState<ProjectTab>("summary");
  const [projectGroup, setProjectGroup] = useState<ProjectGroup>("summary");

  // Tool sub-tab state: null = show the overview; string = tool name of the selected tool detail tab.
  const [toolTab, setToolTab] = useState<string | null>(null);
  // Tool entries loaded for the tools group — shared by the secondary tab bar and content panel.
  const [toolEntries, setToolEntries] = useState<ProjectToolEntry[]>([]);
  const [toolEntriesLoading, setToolEntriesLoading] = useState(false);

  function loadToolEntries() {
    setToolEntriesLoading(true);
    invoke<ProjectToolEntry[]>("list_tools_with_detection")
      .then((data) => { setToolEntries(data); setToolEntriesLoading(false); })
      .catch((err) => { console.error("Failed to load tools:", err); setToolEntriesLoading(false); });
  }

  /** Switch to a group; auto-select first sub-tab (or "summary") */
  function selectGroup(group: ProjectGroup) {
    setProjectGroup(group);
    if (group === "tools") {
      // Switch to tools group: reset to overview (no tool sub-tab selected).
      setToolTab(null);
      setProjectTab("summary");
      loadToolEntries();
      return;
    }
    if (group === "summary") {
      setProjectTab("summary");
    } else {
      const g = PROJECT_GROUPS.find((g) => g.id === group);
      if (g && g.tabs.length > 0) setProjectTab(g.tabs[0]!.id);
    }
  }

  /** Switch to a specific tab and update the group accordingly */
  function selectTab(tab: ProjectTab) {
    setProjectTab(tab);
    setProjectGroup(groupForTab(tab));
    if (tab !== "rules") setCustomRuleEditingIdx(null);
    if (tab !== "commands") setCustomCommandEditingIdx(null);
    if (tab === "activity" && selectedName) {
      loadActivityPage(selectedName, 0);
    }
  }

  // Memory state
  const [memories, setMemories] = useState<Record<string, { value: string; timestamp: string; source: string | null }>>({});
  const [loadingMemories, setLoadingMemories] = useState(false);
  const [buildItems, setBuildItems] = useState<Feature[]>([]);
  const [loadingBuildItems, setLoadingBuildItems] = useState(false);

  // Groups state — names of all groups this project belongs to, and the full
  // list of all available groups (for the "add to group" picker).
  const [projectGroupMemberships, setProjectGroupMemberships] = useState<string[]>([]);
  const [allGroups, setAllGroups] = useState<string[]>([]);
  const [loadingGroups, setLoadingGroups] = useState(false);

  // Recommendations state
  const [recommendations, setRecommendations] = useState<ProjectRecommendation[]>([]);
  const [aiRecsLoading, setAiRecsLoading] = useState(false);
  const [aiRecsLastRunAt, setAiRecsLastRunAt] = useState<string | null>(null);

  // Derived recommendation display values.
  // AI-skill/MCP individual records are collapsed into single rollup cards so the
  // list stays concise — the full suggestions live on the Skills / MCP Servers tabs.
  const normalRecs = recommendations.filter(
    (r) => r.source !== "automatic-ai-skills" && r.source !== "automatic-ai-mcp",
  );
  const aiSkillsRollupCount = recommendations.filter((r) => r.source === "automatic-ai-skills").length;
  const aiMcpRollupCount    = recommendations.filter((r) => r.source === "automatic-ai-mcp").length;
  const recsDisplayCount =
    normalRecs.length +
    (aiSkillsRollupCount > 0 ? 1 : 0) +
    (aiMcpRollupCount > 0 ? 1 : 0);

  // Skills tab AI suggestion state
  const [aiSkillsLoading, setAiSkillsLoading] = useState(false);
  const [aiSkillsSuggestions, setAiSkillsSuggestions] = useState<ProjectRecommendation[]>([]);

  // MCP Servers tab AI suggestion state
  const [aiMcpLoading, setAiMcpLoading] = useState(false);
  const [aiMcpSuggestions, setAiMcpSuggestions] = useState<ProjectRecommendation[]>([]);

  // Activity state
  const [activityEntries, setActivityEntries] = useState<ActivityEntry[]>([]);
  const [loadingActivity, setLoadingActivity] = useState(false);
  // Activity tab pagination (50 per page, 0-based page index)
  const [activityPage, setActivityPage] = useState(0);
  const [activityTotalCount, setActivityTotalCount] = useState(0);
  const [activityPageEntries, setActivityPageEntries] = useState<ActivityEntry[]>([]);
  const [loadingActivityPage, setLoadingActivityPage] = useState(false);
  const ACTIVITY_PAGE_SIZE = 50;

  // Context state
  interface ProjectContextData {
    commands: Record<string, string>;
    entry_points: Record<string, string>;
    concepts: Record<string, { files: string[]; summary: string }>;
    conventions: Record<string, string>;
    gotchas: Record<string, string>;
    docs: Record<string, { path: string; summary: string }>;
  }
  type ProjectDocsData = Record<string, { path: string; summary: string }>;
  const [projectContext, setProjectContext] = useState<ProjectContextData | null>(null);
  const [projectDocs, setProjectDocs] = useState<ProjectDocsData>({});
  const [loadingContext, setLoadingContext] = useState(false);
  // Raw text editor state for context.json
  const [contextRaw, setContextRaw] = useState("");
  const [contextEditing, setContextEditing] = useState(false);
  const [contextDirty, setContextDirty] = useState(false);
  const [contextSaving, setContextSaving] = useState(false);
  const [contextGenerating, setContextGenerating] = useState(false);
  const [contextJsonError, setContextJsonError] = useState<string | null>(null);
  const [contextFileExists, setContextFileExists] = useState(false);

  // Documentation tab state
  // Inline form state for adding a new file/dir path entry
  const [docNewPath, setDocNewPath] = useState("");
  const [docNewPathSummary, setDocNewPathSummary] = useState("");
  // Inline form state for adding a new link entry
  const [docNewLinkUrl, setDocNewLinkUrl] = useState("");
  const [docNewLinkLabel, setDocNewLinkLabel] = useState("");
  // Note editor state
  const [docNoteSelected, setDocNoteSelected] = useState<string | null>(null);
  const [docNoteContent, setDocNoteContent] = useState("");
  const [docNoteDirty, setDocNoteDirty] = useState(false);
  const [docNoteSaving, setDocNoteSaving] = useState(false);
  const [docNoteLoading, setDocNoteLoading] = useState(false);
  const [docNewNoteName, setDocNewNoteName] = useState("");
  const [docNewNoteCreating, setDocNewNoteCreating] = useState(false);
  // Controls whether the inline add-form is visible for each doc sub-tab
  const [showDocPathForm, setShowDocPathForm] = useState(false);
  const [showDocLinkForm, setShowDocLinkForm] = useState(false);

  const fileDocEntries = useMemo(
    () => Object.entries(projectDocs).filter(([, entry]) => !isHttpDocPath(entry.path) && !isManagedDocNotePath(entry.path)),
    [projectDocs],
  );
  const linkDocEntries = useMemo(
    () => Object.entries(projectDocs).filter(([, entry]) => isHttpDocPath(entry.path)),
    [projectDocs],
  );
  // Local skill editing state
  const [localSkillEditing, setLocalSkillEditing] = useState<string | null>(null); // skill name being edited
  const [localSkillContent, setLocalSkillContent] = useState(""); // raw SKILL.md content
  const [localSkillContentCache, setLocalSkillContentCache] = useState<Record<string, string>>({});
  const [localSkillEditName, setLocalSkillEditName] = useState("");
  const [localSkillEditDescription, setLocalSkillEditDescription] = useState("");
  const [localSkillEditBody, setLocalSkillEditBody] = useState("");
  const [localSkillFieldErrors, setLocalSkillFieldErrors] = useState<{ name: string | null; description: string | null }>({ name: null, description: null });
  const [localSkillIsEditing, setLocalSkillIsEditing] = useState(false);
  const [localSkillSaving, setLocalSkillSaving] = useState(false);

  // Custom rule editing state (for inline project rules in the Rules tab)
  const [customRuleEditingIdx, setCustomRuleEditingIdx] = useState<number | null>(null);
  const [customRuleEditName, setCustomRuleEditName] = useState("");
  const [customRuleEditContent, setCustomRuleEditContent] = useState("");
  const [globalRuleContentCache, setGlobalRuleContentCache] = useState<Record<string, string>>({});

  // Custom agent editing state (for project-local agents in the Agents tab)
  const [customAgentEditingIdx, setCustomAgentEditingIdx] = useState<number | null>(null);
  const [customAgentEditName, setCustomAgentEditName] = useState("");
  const [customAgentEditContent, setCustomAgentEditContent] = useState("");

  // Workspace agents state (user_agents from global registry)
  const [availableUserAgents, setAvailableUserAgents] = useState<UserAgentEntry[]>([]);
  const [userAgentAdding, setUserAgentAdding] = useState(false);
  const [userAgentSearch, setUserAgentSearch] = useState("");

  // Workspace commands state (user_commands from global registry)
  const [availableUserCommands, setAvailableUserCommands] = useState<UserCommandEntry[]>([]);
  const [userCommandAdding, setUserCommandAdding] = useState(false);
  const [userCommandSearch, setUserCommandSearch] = useState("");

  // Custom command editing state (for project-local commands)
  const [customCommandEditingIdx, setCustomCommandEditingIdx] = useState<number | null>(null);
  const [customCommandEditName, setCustomCommandEditName] = useState("");
  const [customCommandEditContent, setCustomCommandEditContent] = useState("");

  // Global rule picker state (dropdown add, mirrors SkillSelector pattern)
  const [globalRuleAdding, setGlobalRuleAdding] = useState(false);
  const [globalRuleSearch, setGlobalRuleSearch] = useState("");

  // Editor detection state
  interface EditorInfo { id: string; label: string; installed: boolean; }
  const [installedEditors, setInstalledEditors] = useState<EditorInfo[]>([]);
  const [editorIconPaths, setEditorIconPaths] = useState<Record<string, string>>({});
  const [openInDropdownOpen, setOpenInDropdownOpen] = useState(false);
  const openInDropdownRef = useRef<HTMLDivElement>(null);
  const userAgentDropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadProjects();
    loadAvailableAgents();
    loadAvailableSkills();
    loadAvailableMcpServers();
    loadAvailableTemplates();
    loadAvailableRules();
    loadAvailableProjectTemplates();
    loadAvailableUserAgents();
    loadAvailableUserCommands();
    // Check whether an API key is available through the full resolution chain.
    invoke<boolean>("has_ai_key").then(setHasAnthropicKey).catch(() => setHasAnthropicKey(false));
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

  useEffect(() => {
    const ruleIds = ((project?.file_rules || {})["_project"] || []) as string[];
    if (ruleIds.length === 0) return;

    let cancelled = false;

    async function warmGlobalRuleContent(): Promise<void> {
      for (const ruleId of ruleIds) {
        if (globalRuleContentCache[ruleId] !== undefined) continue;
        try {
          const content: string = await invoke("read_rule", { machineName: ruleId });
          if (!cancelled) {
            setGlobalRuleContentCache((prev) => (prev[ruleId] !== undefined ? prev : { ...prev, [ruleId]: content }));
          }
        } catch {
          if (!cancelled) {
            setGlobalRuleContentCache((prev) => (prev[ruleId] !== undefined ? prev : { ...prev, [ruleId]: "" }));
          }
        }
      }
    }

    void warmGlobalRuleContent();

    return () => {
      cancelled = true;
    };
  }, [globalRuleContentCache, project?.file_rules]);

  useEffect(() => {
    const localSkills = project?.local_skills || [];
    if (!selectedName || localSkills.length === 0) return;

    let cancelled = false;

    async function warmLocalSkillContent(): Promise<void> {
      for (const skillName of localSkills) {
        if (localSkillContentCache[skillName] !== undefined) continue;
        try {
          const content: string = await invoke("read_local_skill", { name: selectedName, skillName });
          const { body } = parseLocalSkillFrontmatter(content);
          if (!cancelled) {
            setLocalSkillContentCache((prev) => (prev[skillName] !== undefined ? prev : { ...prev, [skillName]: body }));
          }
        } catch {
          if (!cancelled) {
            setLocalSkillContentCache((prev) => (prev[skillName] !== undefined ? prev : { ...prev, [skillName]: "" }));
          }
        }
      }
    }

    void warmLocalSkillContent();

    return () => {
      cancelled = true;
    };
  }, [localSkillContentCache, project?.local_skills, selectedName]);

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

  // No auto-select on load — the overview is the landing state.
  // Projects are only selected via explicit user interaction or initialProject prop.

  // Navigate to a specific project when directed from another view (e.g. Agents)
  useEffect(() => {
    if (initialProject && projects.includes(initialProject)) {
      selectProject(initialProject);
      onInitialProjectConsumed?.();
    }
  }, [initialProject, projects]);

  // After a project is selected via initialProject, switch to the requested tab.
  useEffect(() => {
    if (!initialProjectTab) return;
    const validTabs = ["summary", "agents", "skills", "mcp_servers", "groups", "project_file", "rules", "context", "memory", "activity", "recommendations"] as const;
    type ProjectTab = typeof validTabs[number];
    if (validTabs.includes(initialProjectTab as ProjectTab)) {
      selectTab(initialProjectTab as ProjectTab);
    }
    onInitialProjectTabConsumed?.();
  }, [initialProjectTab]);

  // When the parent nav item is clicked while already on the Projects page, reset to the list view.
  useEffect(() => {
    if (resetKey === undefined || resetKey === 0) return;
    setSelectedName(null);
    setIsCreating(false);
  }, [resetKey]);

  // Open the new-project wizard with a template pre-applied.
  // (triggered from the "New project from template" action in ProjectTemplates)
  useEffect(() => {
    if (!initialCreateWithTemplate || availableProjectTemplates.length === 0) return;
    const tmpl = availableProjectTemplates.find((t) => t.name === initialCreateWithTemplate);
    if (!tmpl) return;
    setWizardSourceTemplates([initialCreateWithTemplate]);
    onInitialCreateWithTemplateConsumed?.();
    startCreate({ fromTemplates: [tmpl] });
  }, [initialCreateWithTemplate, availableProjectTemplates]);

  // Reset drift + recommendations state whenever the active project changes
  useEffect(() => {
    setDriftReport(null);
    setCustomRuleEditingIdx(null);
    setGlobalRuleAdding(false);
    setGlobalRuleSearch("");
    setRecommendations([]);
    setAiRecsLastRunAt(null);
    setAiSkillsSuggestions([]);
    setAiMcpSuggestions([]);
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

        // If there are instruction file conflicts, surface the first one so the
        // user can resolve it.  Only show one at a time to avoid overwhelming the UI.
        const conflicts = report.instruction_conflicts ?? [];
        if (conflicts.length > 0) {
          setInstructionConflict((prev) => {
            // Don't replace an already-open conflict dialog.
            if (prev !== null) return prev;
            return conflicts[0]!;
          });
        }
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

  // Clean up any in-progress wizard stub when the component unmounts (e.g. user
  // navigates to a different top-level section via the sidebar).
  useEffect(() => {
    return () => {
      const stub = wizardStubName.current;
      if (stub) {
        // Fire-and-forget: best-effort deletion on unmount. We cannot await here
        // since React cleanup functions must be synchronous.
        invoke("delete_project", { name: stub }).catch(() => {});
        wizardStubName.current = null;
      }
    };
  }, []);

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

  // ── Folder helpers ──────────────────────────────────────────────────────────

  const saveFolders = (updated: ProjectFolder[]) => {
    localStorage.setItem(PROJECT_FOLDERS_KEY, JSON.stringify(updated));
    setFolders(updated);
  };

  const createFolder = () => {
    const id = `folder-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    const name = "New Folder";
    const newFolder: ProjectFolder = { id, name, collapsed: false, projectNames: [] };
    const updated = [...folders, newFolder];
    saveFolders(updated);
    // Immediately start editing the name
    setEditingFolderId(id);
    setEditingFolderName(name);
  };

  const renameFolder = (id: string, name: string) => {
    saveFolders(folders.map((f) => (f.id === id ? { ...f, name } : f)));
  };

  const deleteFolder = (id: string) => {
    // Remove folder but keep its projects (they become ungrouped)
    saveFolders(folders.filter((f) => f.id !== id));
    // Clear the folder filter if the deleted folder was active
    setSelectedFolderId((prev) => (prev === id ? null : prev));
  };

  const toggleFolderCollapsed = (id: string) => {
    saveFolders(folders.map((f) => (f.id === id ? { ...f, collapsed: !f.collapsed } : f)));
  };

  const moveProjectToFolder = (projectName: string, folderId: string | null) => {
    // Remove from all folders first
    const cleaned = folders.map((f) => ({
      ...f,
      projectNames: f.projectNames.filter((n) => n !== projectName),
    }));
    if (folderId === null) {
      saveFolders(cleaned);
      return;
    }
    saveFolders(cleaned.map((f) => (f.id === folderId ? { ...f, projectNames: [...f.projectNames, projectName] } : f)));
  };

  /** Returns project names not assigned to any folder, in current display order. */
  const ungroupedProjects = projects.filter((n) => !folders.some((f) => f.projectNames.includes(n)));

  // ── Folder drag-onto state ──────────────────────────────────────────────────
  const [dragOverFolderId, setDragOverFolderId] = useState<string | null>(null);
  /** Within-folder drop target: folderId + insertion index within that folder's projectNames. */
  const [folderDropTarget, setFolderDropTarget] = useState<{ folderId: string; itemIdx: number } | null>(null);
  /** Ref to the sidebar scroll container, used for drop-to-ungrouped zone detection. */
  const sidebarListRef = useRef<HTMLDivElement>(null);
  /** Non-null when a folder header is being dragged for reordering. */
  const [draggingFolderId, setDraggingFolderId] = useState<string | null>(null);
  /** Insertion index among folders while a folder is being dragged. */
  const [folderReorderDropIdx, setFolderReorderDropIdx] = useState<number | null>(null);
  const folderReorderDropIdxRef = useRef<number | null>(null);

  // Compute which ungrouped-project drop index the pointer is over.
  // Uses [data-ungrouped-idx] attributes so folder <li> elements are ignored.
  const getDropIndex = (clientY: number): number | null => {
    if (!listRef.current) return null;
    const items = Array.from(
      listRef.current.querySelectorAll<HTMLElement>("[data-ungrouped-idx]")
    );
    for (let i = 0; i < items.length; i++) {
      const rect = items[i]!.getBoundingClientRect();
      const midY = rect.top + rect.height / 2;
      if (clientY < midY) return i;
    }
    return items.length;
  };

  // ── Drag destination resolver ────────────────────────────────────────────────
  // Walk up the DOM from a point element to determine which drag zone the pointer
  // is over: a folder header, a folder item row, an ungrouped item, or the
  // generic ungrouped zone (sidebar background).
  type DragDest =
    | { kind: "folder-header"; folderId: string }
    | { kind: "folder-item"; folderId: string; itemIdx: number }
    | { kind: "ungrouped"; ungroupedIdx: number }
    | { kind: "ungrouped-zone" }
    | { kind: "folder-reorder"; folderIdx: number }
    | null;

  const resolveDragDest = (clientX: number, clientY: number): DragDest => {
    const el = document.elementFromPoint(clientX, clientY);
    if (!el) return null;
    // Walk up from the hit element looking for data attributes
    let cur: HTMLElement | null = el as HTMLElement;
    while (cur) {
      const ds = cur.dataset;
      // Folder reorder zone (on folder <li> with data-folder-reorder-idx)
      if (ds.folderReorderIdx !== undefined) {
        const idx = parseInt(ds.folderReorderIdx, 10);
        if (!isNaN(idx)) return { kind: "folder-reorder", folderIdx: idx };
      }
      // Folder item row inside a folder
      if (ds.folderItemFid && ds.folderItemIdx !== undefined) {
        const itemIdx = parseInt(ds.folderItemIdx, 10);
        if (!isNaN(itemIdx)) {
          // Decide above/below based on pointer Y vs midpoint
          const rect = cur.getBoundingClientRect();
          const mid = rect.top + rect.height / 2;
          return { kind: "folder-item", folderId: ds.folderItemFid, itemIdx: clientY < mid ? itemIdx : itemIdx + 1 };
        }
      }
      // Folder header (drop onto folder)
      if (ds.folderId) {
        return { kind: "folder-header", folderId: ds.folderId };
      }
      // Ungrouped item
      if (ds.ungroupedIdx !== undefined) {
        const idx = parseInt(ds.ungroupedIdx, 10);
        if (!isNaN(idx)) {
          const rect = cur.getBoundingClientRect();
          const mid = rect.top + rect.height / 2;
          return { kind: "ungrouped", ungroupedIdx: clientY < mid ? idx : idx + 1 };
        }
      }
      // Ungrouped zone (sidebar scroll container or the <ul>)
      if (ds.ungroupedZone !== undefined) {
        return { kind: "ungrouped-zone" };
      }
      cur = cur.parentElement;
    }
    return null;
  };

  /**
   * Pointer-events drag handler for project items.
   * @param projectName - Name of the project being dragged
   * @param sourceFolderId - Folder ID the project lives in, or null if ungrouped
   */
  const handleGripDown = (projectName: string, sourceFolderId: string | null, e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // For ungrouped items we still track dragIdx/dropIdx for backward compat
    const ungroupedIdx = sourceFolderId === null ? ungroupedProjects.indexOf(projectName) : null;
    if (ungroupedIdx !== null) {
      dragIdxRef.current = ungroupedIdx;
      dropIdxRef.current = ungroupedIdx;
      setDragIdx(ungroupedIdx);
      setDropIdx(ungroupedIdx);
    } else {
      dragIdxRef.current = null;
      dropIdxRef.current = null;
      setDragIdx(null);
      setDropIdx(null);
    }
    setDragGhost({ name: projectName, x: e.clientX, y: e.clientY });

    const onMove = (ev: PointerEvent) => {
      // Update ghost position
      setDragGhost({ name: projectName, x: ev.clientX, y: ev.clientY });

      const dest = resolveDragDest(ev.clientX, ev.clientY);

      // Reset all indicators, then set the relevant ones
      setDragOverFolderId(null);
      setFolderDropTarget(null);

      if (!dest) {
        // No recognized zone — keep previous ungrouped drop idx
        const target = getDropIndex(ev.clientY);
        dropIdxRef.current = target;
        setDropIdx(target);
        return;
      }

      switch (dest.kind) {
        case "folder-header":
          setDragOverFolderId(dest.folderId);
          break;
        case "folder-item":
          setFolderDropTarget({ folderId: dest.folderId, itemIdx: dest.itemIdx });
          break;
        case "ungrouped": {
          const target = dest.ungroupedIdx;
          dropIdxRef.current = target;
          setDropIdx(target);
          break;
        }
        case "ungrouped-zone": {
          // Drop to end of ungrouped list
          const target = ungroupedProjects.length;
          dropIdxRef.current = target;
          setDropIdx(target);
          break;
        }
      }
    };

    const onUp = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);

      setDragGhost(null);
      setDragOverFolderId(null);
      setFolderDropTarget(null);

      const dest = resolveDragDest(ev.clientX, ev.clientY);

      const fromUngroupedIdx = dragIdxRef.current;
      const toUngroupedIdx = dropIdxRef.current;
      dragIdxRef.current = null;
      dropIdxRef.current = null;
      setDragIdx(null);
      setDropIdx(null);

      if (!dest) return;

      switch (dest.kind) {
        case "folder-header":
          // Move project into this folder (appended at end)
          moveProjectToFolder(projectName, dest.folderId);
          return;

        case "folder-item": {
          // Move project into the folder at a specific position
          // First remove from all folders
          const cleaned = folders.map((f) => ({
            ...f,
            projectNames: f.projectNames.filter((n) => n !== projectName),
          }));
          // Insert at the correct position
          const updated = cleaned.map((f) => {
            if (f.id !== dest.folderId) return f;
            const names = [...f.projectNames];
            names.splice(dest.itemIdx, 0, projectName);
            return { ...f, projectNames: names };
          });
          saveFolders(updated);
          return;
        }

        case "ungrouped":
        case "ungrouped-zone": {
          // If it was in a folder, remove from folder first
          if (sourceFolderId) {
            moveProjectToFolder(projectName, null);
          }
          // Reorder within ungrouped list
          if (sourceFolderId === null && fromUngroupedIdx !== null && toUngroupedIdx !== null && fromUngroupedIdx !== toUngroupedIdx) {
            setProjects((prev) => {
              // Map ungrouped indices back to the full projects array indices
              const ungrouped = prev.filter((n) => !folders.some((f) => f.projectNames.includes(n)));
              const fromName = ungrouped[fromUngroupedIdx];
              const toInsertBefore = toUngroupedIdx < ungrouped.length ? ungrouped[toUngroupedIdx] : null;
              if (!fromName) return prev;
              const without = prev.filter((n) => n !== fromName);
              if (toInsertBefore === null) {
                // Insert at end
                const reordered = [...without, fromName];
                saveProjectOrder(reordered);
                return reordered;
              }
              const insertIdx = without.indexOf(toInsertBefore);
              const reordered = [...without.slice(0, insertIdx), fromName, ...without.slice(insertIdx)];
              saveProjectOrder(reordered);
              return reordered;
            });
          }
          return;
        }
      }
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  /** Pointer-events drag handler for reordering folder headers. */
  const handleFolderGripDown = (folderId: string, e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const startFolderIdx = folders.findIndex((f) => f.id === folderId);
    setDraggingFolderId(folderId);
    setFolderReorderDropIdx(startFolderIdx);
    folderReorderDropIdxRef.current = startFolderIdx;
    const folderName = folders[startFolderIdx]?.name ?? "Folder";
    setDragGhost({ name: folderName, x: e.clientX, y: e.clientY });

    const updateDropIdx = (idx: number) => {
      folderReorderDropIdxRef.current = idx;
      setFolderReorderDropIdx(idx);
    };

    const onMove = (ev: PointerEvent) => {
      setDragGhost({ name: folderName, x: ev.clientX, y: ev.clientY });
      // Walk up from hit element to find a [data-folder-reorder-idx] ancestor
      const el = document.elementFromPoint(ev.clientX, ev.clientY);
      let cur: HTMLElement | null = el as HTMLElement;
      while (cur) {
        if (cur.dataset?.folderReorderIdx !== undefined) {
          const idx = parseInt(cur.dataset.folderReorderIdx, 10);
          if (!isNaN(idx)) {
            const rect = cur.getBoundingClientRect();
            const mid = rect.top + rect.height / 2;
            updateDropIdx(ev.clientY < mid ? idx : idx + 1);
            return;
          }
        }
        cur = cur.parentElement;
      }
    };

    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);

      setDragGhost(null);
      const fromIdx = startFolderIdx;
      const toIdx = folderReorderDropIdxRef.current;
      folderReorderDropIdxRef.current = null;
      setDraggingFolderId(null);
      setFolderReorderDropIdx(null);

      if (toIdx === null || fromIdx === toIdx || fromIdx === toIdx - 1) return;

      const reordered = [...folders];
      const [removed] = reordered.splice(fromIdx, 1);
      const insertAt = toIdx > fromIdx ? toIdx - 1 : toIdx;
      reordered.splice(insertAt, 0, removed);
      saveFolders(reordered);
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
      // Populate the overview details map from the stored config only.
      // Autodetected items that differ from the stored config are drift —
      // they are surfaced when the user opens the project, not silently merged.
      const entries = await Promise.all(
        ordered.map(async (name) => {
          try {
            const raw: string = await invoke("read_project", { name });
            return [name, JSON.parse(raw) as Project] as const;
          } catch {
            return null;
          }
        })
      );
      setProjectDetailsMap(new Map(entries.filter(Boolean) as [string, Project][]));
    } catch (err: any) {
      setError(`Failed to load projects: ${err}`);
    } finally {
      setProjectsLoading(false);
    }
  };

  const loadAvailableAgents = async () => {
    try {
      const result: AgentInfo[] = await invoke("list_agents");
      result.sort((a, b) => a.label.localeCompare(b.label));
      setAvailableAgents(result);
    } catch {
      // Agents list may not be available yet
    }
  };

  const loadAvailableSkills = async () => {
    try {
      const result: { name: string; sources: string[]
 }[] = await invoke("get_skills");
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

  const loadAvailableUserAgents = async () => {
    try {
      const result: UserAgentEntry[] = await invoke("get_user_agents");
      setAvailableUserAgents(result.sort((a, b) => a.name.localeCompare(b.name)));
    } catch {
      // User agents may not exist yet
    }
  };

  const loadAvailableUserCommands = async () => {
    try {
      const result: UserCommandEntry[] = await invoke("get_user_commands");
      setAvailableUserCommands(result.sort((a, b) => a.id.localeCompare(b.id)));
    } catch {
      // Commands may not exist yet
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

  /** Toggle a template's selection in the multi-select picker without immediately applying it. */
  const toggleProjectTemplateSelection = (tmplName: string) => {
    setSelectedProjectTemplates((prev) =>
      prev.includes(tmplName) ? prev.filter((n) => n !== tmplName) : [...prev, tmplName]
    );
  };

  /**
   * Merge all currently-selected templates into the open project and close the picker.
   * Each template's assets are unioned in; unified instructions are concatenated.
   */
  const applySelectedProjectTemplates = () => {
    if (!project || selectedProjectTemplates.length === 0) return;
    const templates = availableProjectTemplates.filter((t) => selectedProjectTemplates.includes(t.name));

    let mergedAgents = [...project.agents];
    let mergedSkills = [...project.skills];
    let mergedMcpServers = [...project.mcp_servers];
    let mergedProviders = [...project.providers];
    let mergedDescription = project.description;
    let anyUnified = false;
    const pendingEntries: { content: string; rules: string[] }[] = [];

    for (const tmpl of templates) {
      mergedAgents = [...new Set([...mergedAgents, ...tmpl.agents])];
      mergedSkills = [...new Set([...mergedSkills, ...tmpl.skills])];
      mergedMcpServers = [...new Set([...mergedMcpServers, ...tmpl.mcp_servers])];
      mergedProviders = [...new Set([...mergedProviders, ...tmpl.providers])];
      if (!mergedDescription) mergedDescription = tmpl.description;
      const hasContent = !!(tmpl.unified_instruction && tmpl.unified_instruction.trim());
      const hasRules = (tmpl.unified_rules || []).length > 0;
      if (hasContent || hasRules) {
        anyUnified = true;
        pendingEntries.push({ content: tmpl.unified_instruction || "", rules: tmpl.unified_rules || [] });
      }
    }

    setProject({
      ...project,
      description: mergedDescription,
      agents: mergedAgents,
      skills: mergedSkills,
      mcp_servers: mergedMcpServers,
      providers: mergedProviders,
      ...(anyUnified ? { instruction_mode: "unified" } : {}),
    });

    if (anyUnified) {
      pendingUnifiedInstruction.current = pendingEntries;
    }

    setDirty(true);
    setShowProjectTemplatePicker(false);
    // selectedProjectTemplates intentionally kept so the panel shows what was applied
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

  /** Load which groups this project belongs to, and all available groups. */
  const loadGroups = async (projectName: string) => {
    try {
      setLoadingGroups(true);
      // Clear stale data immediately so UI doesn't show previous project's groups.
      setProjectGroupMemberships([]);
      const [memberships, available] = await Promise.all([
        invoke<string[]>("groups_for_project", { projectName }),
        invoke<string[]>("list_groups"),
      ]);
      setProjectGroupMemberships(memberships);
      setAllGroups(available.sort((a, b) => a.localeCompare(b)));
    } catch (err: any) {
      console.error("Failed to load groups:", err);
      setError(`Failed to load groups: ${err}`);
    } finally {
      setLoadingGroups(false);
    }
  };

  /** Add this project to a group, save the group, then re-sync ALL projects
   *  in the group so every project's peer list is updated. */
  const handleAddToGroup = async (groupName: string, projectName: string) => {
    try {
      const raw: string = await invoke("read_group", { name: groupName });
      const g = JSON.parse(raw);
      if (!g.projects.includes(projectName)) {
        g.projects.push(projectName);
        g.updated_at = new Date().toISOString();
        await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
        setProjectGroupMemberships((prev) => [...prev, groupName].sort((a, b) => a.localeCompare(b)));
        // Sync ALL projects in the group - each one's peer list changes.
        for (const name of g.projects) {
          invoke("sync_project", { name }).catch((e: unknown) => {
            console.warn(`Group sync: could not sync project '${name}':`, e);
          });
        }
      }
    } catch (err: any) {
      setError(`Failed to add to group: ${err}`);
    }
  };

  /** Remove this project from a group, save, then re-sync ALL remaining projects
   *  (peer lists change) and the removed project (to strip its group block). */
  const handleRemoveFromGroup = async (groupName: string, projectName: string) => {
    try {
      const raw: string = await invoke("read_group", { name: groupName });
      const g = JSON.parse(raw);
      g.projects = g.projects.filter((p: string) => p !== projectName);
      g.updated_at = new Date().toISOString();
      await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
      setProjectGroupMemberships((prev) => prev.filter((n) => n !== groupName));
      // Sync remaining projects (peer lists change) and the removed project.
      const toSync = [...g.projects, projectName];
      for (const name of toSync) {
        invoke("sync_project", { name }).catch((e: unknown) => {
          console.warn(`Group sync: could not sync project '${name}':`, e);
        });
      }
    } catch (err: any) {
      setError(`Failed to remove from group: ${err}`);
    }
  };

  const handleRemoveFromAllGroups = async (projectName: string) => {
    const confirmed = await ask(
      `Remove "${projectName}" from all ${projectGroupMemberships.length} group${projectGroupMemberships.length === 1 ? "" : "s"}?`,
      { title: "Remove from All Groups", kind: "warning" }
    );
    if (!confirmed) return;

    // Collect all projects that need to be synced (peers in each group + the removed project)
    const toSync = new Set<string>([projectName]);
    
    for (const groupName of projectGroupMemberships) {
      try {
        const raw: string = await invoke("read_group", { name: groupName });
        const g = JSON.parse(raw);
        g.projects = g.projects.filter((p: string) => p !== projectName);
        g.updated_at = new Date().toISOString();
        await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
        // Add remaining projects in this group - their peer lists need updating
        for (const peer of g.projects) {
          toSync.add(peer);
        }
      } catch (err: any) {
        console.error(`Failed to remove from group ${groupName}:`, err);
      }
    }
    setProjectGroupMemberships([]);
    // Sync all affected projects
    for (const name of toSync) {
      invoke("sync_project", { name }).catch((e: unknown) => {
        console.warn(`Group sync: could not sync project '${name}':`, e);
      });
    }
  };

  const loadContext = async (projectName: string) => {
    try {
      setLoadingContext(true);
      const [parsedRaw, rawText, docsRaw] = await Promise.all([
        invoke<string>("get_project_context", { name: projectName }),
        invoke<string>("read_project_context_raw", { name: projectName }),
        invoke<string>("get_project_docs", { name: projectName }),
      ]);
      setProjectContext(JSON.parse(parsedRaw));
      setProjectDocs(JSON.parse(docsRaw));
      setContextRaw(rawText);
      setContextFileExists(rawText.length > 0);
      setContextEditing(false);
      setContextDirty(false);
      setContextJsonError(null);
    } catch (err: any) {
      console.error("Failed to load project context:", err);
      setProjectContext(null);
      setProjectDocs({});
      setContextRaw("");
      setContextFileExists(false);
    } finally {
      setLoadingContext(false);
    }
  };

  const handleSaveContext = async () => {
    if (!selectedName) return;
    try {
      JSON.parse(contextRaw);
    } catch (e: any) {
      setContextJsonError(`Invalid JSON: ${e.message}`);
      return;
    }
    setContextSaving(true);
    setContextJsonError(null);
    try {
      await invoke("save_project_context_raw", { name: selectedName, content: contextRaw });
      setContextDirty(false);
      setContextEditing(false);
      setContextFileExists(true);
      const [parsed, docsRaw]: [string, string] = await Promise.all([
        invoke<string>("get_project_context", { name: selectedName }),
        invoke<string>("get_project_docs", { name: selectedName }),
      ]);
      setProjectContext(JSON.parse(parsed));
      setProjectDocs(JSON.parse(docsRaw));
      notifyProjectUpdated();
    } catch (err: any) {
      setContextJsonError(`${err}`);
    } finally {
      setContextSaving(false);
    }
  };

  const handleGenerateContext = async () => {
    if (!selectedName) return;
    setContextGenerating(true);
    setContextJsonError(null);
    const entryId = log(`Analysing project "${selectedName}"…`, "running");
    try {
      const generated: string = await invoke("ai_generate_context", { name: selectedName });
      // Pretty-print the returned JSON before putting it in the editor.
      const pretty = JSON.stringify(JSON.parse(generated), null, 2);
      setContextRaw(pretty);
      setContextEditing(true);
      setContextDirty(true);
      update(entryId, `Context generated for "${selectedName}" — review and save`, "success");
    } catch (err: any) {
      setContextJsonError(`Generation failed: ${err}`);
      update(entryId, `Context generation failed: ${err}`, "error");
    } finally {
      setContextGenerating(false);
    }
  };

  // ── Documentation tab helpers ─────────────────────────────────────────────

  /**
   * Return the docs index loaded from `.automatic/docs.json`.
   */
  const parsedDocs = (): ProjectDocsData => projectDocs;

  /**
   * Persist an updated docs index to `.automatic/docs.json`.
   */
  const saveDocsToContext = async (
    newDocs: ProjectDocsData
  ): Promise<void> => {
    if (!selectedName) return;
    const updated = JSON.stringify(newDocs, null, 2);
    await invoke("save_project_docs_raw", { name: selectedName, content: updated });
    setProjectDocs(newDocs);
    setProjectContext((prev) => (prev ? { ...prev, docs: newDocs } : prev));
  };

  /** Add or update a file/dir entry in the docs map. */
  const addDocPath = async (path: string, summary: string): Promise<void> => {
    if (!path.trim()) return;
    const docs = parsedDocs();
    // Use the basename as the key (de-duplicating with a suffix if needed)
    const base = path.split("/").pop() ?? path;
    const key = docs[base] ? `${base}_${Date.now()}` : base;
    await saveDocsToContext({ ...docs, [key]: { path: path.trim(), summary: summary.trim() } });
  };

  /** Add or update a link entry in the docs map. */
  const addDocLink = async (url: string, label: string): Promise<void> => {
    if (!url.trim()) return;
    const docs = parsedDocs();
    const key = (label.trim() || url.trim().replace(/https?:\/\//, "").split("/")[0]) ?? "link";
    const safeKey = docs[key] ? `${key}_${Date.now()}` : key;
    await saveDocsToContext({ ...docs, [safeKey]: { path: url.trim(), summary: label.trim() } });
  };

  const handleBrowseDocPath = async (): Promise<void> => {
    const picked: string | null = await invoke("open_directory_dialog");
    if (picked) setDocNewPath(picked);
  };

  const handleAddDocPath = async (): Promise<void> => {
    if (!docNewPath.trim()) return;
    await addDocPath(docNewPath, docNewPathSummary);
    setDocNewPath("");
    setDocNewPathSummary("");
    setShowDocPathForm(false);
  };

  const handleAddDocLink = async (): Promise<void> => {
    if (!docNewLinkUrl.trim()) return;
    await addDocLink(docNewLinkUrl, docNewLinkLabel);
    setDocNewLinkUrl("");
    setDocNewLinkLabel("");
    setShowDocLinkForm(false);
  };

  /** Remove a doc entry by key. Also deletes the note file if it's a note entry. */
  const removeDocEntry = async (key: string, isNote: boolean): Promise<void> => {
    if (!selectedName) return;
    const docs = parsedDocs();
    const { [key]: _removed, ...rest } = docs;
    await saveDocsToContext(rest);
    if (isNote) {
      try {
        await invoke("delete_doc_note", { name: selectedName, noteName: key + ".md" });
      } catch {
        // best-effort — file may not exist yet
      }
      if (docNoteSelected === key) {
        setDocNoteSelected(null);
        setDocNoteContent("");
        setDocNoteDirty(false);
      }
    }
  };

  /** Load the content of a note file into the editor. */
  const loadDocNote = async (key: string): Promise<void> => {
    if (!selectedName) return;
    setDocNoteLoading(true);
    setDocNoteSelected(key);
    setDocNoteDirty(false);
    try {
      const content: string = await invoke("read_doc_note", {
        name: selectedName,
        noteName: key + ".md",
      });
      setDocNoteContent(content);
    } catch (err) {
      console.error("Failed to load doc note:", err);
      setDocNoteContent("");
    } finally {
      setDocNoteLoading(false);
    }
  };

  /** Save the current note editor content to disk. */
  const saveDocNote = async (): Promise<void> => {
    if (!selectedName || !docNoteSelected) return;
    setDocNoteSaving(true);
    try {
      await invoke("save_doc_note", {
        name: selectedName,
        noteName: docNoteSelected + ".md",
        content: docNoteContent,
      });
      setDocNoteDirty(false);
    } catch (err) {
      console.error("Failed to save doc note:", err);
    } finally {
      setDocNoteSaving(false);
    }
  };

  /** Create a new note: adds an index entry to docs.json, then opens the editor. */
  const createDocNote = async (noteName: string): Promise<void> => {
    if (!noteName.trim() || !selectedName) return;
    // Sanitise: lowercase, spaces → hyphens, strip non-alphanumeric except hyphens
    const slug = noteName
      .trim()
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "");
    if (!slug) return;
    const docs = parsedDocs();
    if (docs[slug]) {
      // Already exists — just select it
      await loadDocNote(slug);
      return;
    }
    await saveDocsToContext({
      ...docs,
      [slug]: { path: `.automatic/docs/${slug}.md`, summary: noteName.trim() },
    });
    setDocNoteContent("");
    setDocNoteDirty(false);
    setDocNoteSelected(slug);
    setDocNewNoteName("");
    setDocNewNoteCreating(false);
  };

  // ── End documentation tab helpers ────────────────────────────────────────

  // Remove a recommendation from all local state arrays and notify the global
  // Recommendations view to re-fetch. Call this after any dismiss or action so
  // the rollup counts (badge, Recommendations tab) stay accurate.
  const removeRecommendation = (id: number) => {
    setRecommendations((prev) => prev.filter((r) => r.id !== id));
    setAiSkillsSuggestions((prev) => prev.filter((r) => r.id !== id));
    setAiMcpSuggestions((prev) => prev.filter((r) => r.id !== id));
    window.dispatchEvent(new CustomEvent("recommendations-updated"));
  };

  const loadRecommendations = async (projectName: string) => {
    try {
      const [recs, skillRecs, mcpRecs] = await Promise.all([
        invoke<ProjectRecommendation[]>("evaluate_project_recommendations", { project: projectName }),
        invoke<ProjectRecommendation[]>("list_recommendations_by_source", { project: projectName, source: "automatic-ai-skills" }),
        invoke<ProjectRecommendation[]>("list_recommendations_by_source", { project: projectName, source: "automatic-ai-mcp" }),
      ]);
      setRecommendations(recs);
      setAiSkillsSuggestions(skillRecs);
      setAiMcpSuggestions(mcpRecs);
      // Fetch the last AI run timestamp (non-blocking, best-effort).
      invoke<string | null>("get_ai_recommendations_timestamp", { project: projectName })
        .then((ts) => setAiRecsLastRunAt(ts ?? null))
        .catch(() => {});
      // Notify the global Recommendations view so it re-fetches from the DB.
      window.dispatchEvent(new CustomEvent("recommendations-updated"));
    } catch (err: any) {
      console.error("Failed to evaluate recommendations:", err);
      // Non-fatal — clear so stale data isn't shown
      setRecommendations([]);
      setAiSkillsSuggestions([]);
      setAiMcpSuggestions([]);
    }
  };

  const handleUpdateAiRecommendations = async () => {
    if (!selectedName || aiRecsLoading) return;
    setAiRecsLoading(true);
    const entryId = log(`Analysing recommendations for "${selectedName}"…`, "running");
    try {
      const result = await invoke<{ recommendations: ProjectRecommendation[]; last_run_at: string }>(
        "ai_generate_project_recommendations",
        { project: selectedName, force: true },
      );
      setRecommendations(result.recommendations);
      setAiRecsLastRunAt(result.last_run_at);
      window.dispatchEvent(new CustomEvent("recommendations-updated"));
      update(entryId, `Recommendations updated for "${selectedName}"`, "success");
    } catch (err: any) {
      console.error("Failed to generate AI recommendations:", err);
      update(entryId, `Recommendation analysis failed: ${err}`, "error");
    } finally {
      setAiRecsLoading(false);
    }
  };

  const handleSuggestSkills = async () => {
    if (!selectedName || aiSkillsLoading) return;
    setAiSkillsLoading(true);
    const entryId = log(`Suggesting skills for "${selectedName}"…`, "running");
    try {
      const recs = await invoke<ProjectRecommendation[]>("ai_suggest_skills", { project: selectedName });
      const skillRecs = recs.filter((r) => r.source === "automatic-ai-skills" && r.status === "pending");
      setAiSkillsSuggestions(skillRecs);
      window.dispatchEvent(new CustomEvent("recommendations-updated"));
      update(entryId, `Skills suggestions ready for "${selectedName}"`, "success");
    } catch (err: any) {
      console.error("Failed to suggest skills:", err);
      update(entryId, `Skills suggestion failed: ${err}`, "error");
    } finally {
      setAiSkillsLoading(false);
    }
  };

  const handleSuggestMcpServers = async () => {
    if (!selectedName || aiMcpLoading) return;
    setAiMcpLoading(true);
    const entryId = log(`Suggesting MCP servers for "${selectedName}"…`, "running");
    try {
      const recs = await invoke<ProjectRecommendation[]>("ai_suggest_mcp_servers", { project: selectedName });
      const mcpRecs = recs.filter((r) => r.source === "automatic-ai-mcp" && r.status === "pending");
      setAiMcpSuggestions(mcpRecs);
      window.dispatchEvent(new CustomEvent("recommendations-updated"));
      update(entryId, `MCP server suggestions ready for "${selectedName}"`, "success");
    } catch (err: any) {
      console.error("Failed to suggest MCP servers:", err);
      update(entryId, `MCP server suggestion failed: ${err}`, "error");
    } finally {
      setAiMcpLoading(false);
    }
  };

  // Re-evaluate recommendations whenever any project mutation occurs.
  // Callers signal a change by calling notifyProjectUpdated() — no need to
  // wire loadRecommendations into every individual save handler.
  useEffect(() => {
    if (projectVersion === 0 || !selectedName) return;
    loadRecommendations(selectedName);
  }, [projectVersion]); // eslint-disable-line react-hooks/exhaustive-deps

  const loadActivity = async (projectName: string) => {
    try {
      setLoadingActivity(true);
      const raw: string = await invoke("get_project_activity", { project: projectName, limit: 5 });
      setActivityEntries(JSON.parse(raw) as ActivityEntry[]);
    } catch (err: any) {
      console.error("Failed to load activity:", err);
    } finally {
      setLoadingActivity(false);
    }
  };

  const loadActivityPage = async (projectName: string, page: number) => {
    try {
      setLoadingActivityPage(true);
      const offset = page * ACTIVITY_PAGE_SIZE;
      const [raw, count] = await Promise.all([
        invoke<string>("get_project_activity_paged", {
          project: projectName,
          limit: ACTIVITY_PAGE_SIZE,
          offset,
        }),
        invoke<number>("get_project_activity_count", { project: projectName }),
      ]);
      setActivityPageEntries(JSON.parse(raw) as ActivityEntry[]);
      setActivityTotalCount(count);
      setActivityPage(page);
    } catch (err: any) {
      console.error("Failed to load activity page:", err);
    } finally {
      setLoadingActivityPage(false);
    }
  };

  const loadBuildItems = async (projectName: string) => {
    try {
      setLoadingBuildItems(true);
      const result = await invoke<Feature[]>("list_features", { project: projectName });
      const recent = [...result]
        .sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime())
        .slice(0, 4);
      setBuildItems(recent);
    } catch (err: any) {
      console.error("Failed to load build items:", err);
      setBuildItems([]);
    } finally {
      setLoadingBuildItems(false);
    }
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
    if (!selectedName || !activeProjectFile || !project) return;
    setProjectFileSaving(true);
    try {
      // Flush the in-memory project config (including file_rules) to disk first,
      // so save_project_file on the backend reads up-to-date rule assignments.
      // This also handles the case where rules were toggled on a not-yet-existing file.
      const toSave = { ...project, name: selectedName, updated_at: new Date().toISOString() };
      await invoke("save_project", { name: selectedName, data: JSON.stringify(toSave, null, 2) });
      setDirty(false);

      await invoke("save_project_file", {
        name: selectedName,
        filename: activeProjectFile,
        content: projectFileContent,
      });
      setProjectFileDirty(false);

      // Reload file list so the "exists" flag updates for newly created files
      await loadProjectFiles(selectedName);
      notifyProjectUpdated();
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
      setShowTemplatePicker(false);
    } catch (err: any) {
      setError(`Failed to load template: ${err}`);
    }
  };

  const handleGenerateInstruction = async () => {
    if (!selectedName || !activeProjectFile) return;
    setProjectFileGenerating(true);
    // Resolve a human-readable label: use the agent name(s) rather than the
    // internal "_unified" virtual filename.
    const fileInfo = projectFiles.find((f) => f.filename === activeProjectFile);
    const displayLabel =
      activeProjectFile === "_unified"
        ? (fileInfo?.agents?.join(" & ") ?? "shared instruction file")
        : activeProjectFile;
    const entryId = log(`Generating instruction file for ${displayLabel}…`, "running");
    try {
      const generated: string = await invoke("ai_generate_instruction", {
        name: selectedName,
        filename: activeProjectFile,
      });
      setProjectFileContent(generated);
      setProjectFileEditing(true);
      setProjectFileDirty(true);
      update(entryId, `Instruction file for ${displayLabel} generated — review and save`, "success");
    } catch (err: any) {
      update(entryId, `Instruction generation failed: ${err}`, "error");
    } finally {
      setProjectFileGenerating(false);
    }
  };

  const handleUpdateInstruction = async () => {
    if (!selectedName || !activeProjectFile) return;
    if (!projectFileContent.trim()) return;
    setProjectFileUpdating(true);
    const fileInfo = projectFiles.find((f) => f.filename === activeProjectFile);
    const displayLabel =
      activeProjectFile === "_unified"
        ? (fileInfo?.agents?.join(" & ") ?? "shared instruction file")
        : activeProjectFile;
    const entryId = log(`Updating instruction file for ${displayLabel}…`, "running");
    try {
      const updated: string = await invoke("ai_update_instruction", {
        name: selectedName,
        filename: activeProjectFile,
        currentContent: projectFileContent,
      });
      setProjectFileContent(updated);
      setProjectFileEditing(true);
      setProjectFileDirty(true);
      update(entryId, `Instruction file for ${displayLabel} updated — review and save`, "success");
    } catch (err: any) {
      update(entryId, `Instruction update failed: ${err}`, "error");
    } finally {
      setProjectFileUpdating(false);
    }
  };

  const selectProject = async (name: string) => {
    // If the wizard is open, cancel it (cleans up any saved stub) before loading the selected project.
    if (isCreating) {
      await cancelCreate();
    }
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
        disabled_mcp_servers: stored.disabled_mcp_servers || [],
        providers: stored.providers || [],
        agents: [...storedAgents, ...newAgents],
        created_at: stored.created_at || new Date().toISOString(),
        updated_at: stored.updated_at || new Date().toISOString(),
        file_rules: stored.file_rules || {},
        instruction_mode: stored.instruction_mode || "per-agent",
        agent_options: stored.agent_options,
        custom_rules: stored.custom_rules || [],
        tools: stored.tools || [],
        custom_agents: stored.custom_agents || [],
        user_agents: stored.user_agents || [],
        custom_commands: stored.custom_commands || [],
        user_commands: stored.user_commands || [],
      };

      setSelectedName(name);
      localStorage.setItem(LAST_PROJECT_KEY, name);
      setProject(data);
      setProjectDetailsMap((prev) => new Map(prev).set(name, data));
      setDirty(detectedDiffers);
      setIsCreating(false);
      setError(null);
      if (!sidebarPinned) {
        setSidebarExpanded(false);
      }
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
      await loadGroups(name);
      await loadActivity(name);
      await loadRecommendations(name);
      await loadContext(name);
      await loadBuildItems(name);
      // Reset activity tab pagination for the newly selected project
      setActivityPage(0);
      setActivityPageEntries([]);
      setActivityTotalCount(0);
      // Reset tools group state so a stale sub-tab from a previous project isn't shown
      setToolTab(null);
      setToolEntries([]);
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

  // Reload project state from disk and refresh all dependent UI.
  // Always re-affirms selectedName so that any async state race between
  // isCreating=false and the reload completing cannot drop back to the overview.
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
        disabled_mcp_servers: parsed.disabled_mcp_servers || [],
        providers: parsed.providers || [],
        agents: parsed.agents || [],
        created_at: parsed.created_at || new Date().toISOString(),
        updated_at: parsed.updated_at || new Date().toISOString(),
        file_rules: parsed.file_rules || {},
        instruction_mode: parsed.instruction_mode || "per-agent",
        agent_options: parsed.agent_options,
        custom_rules: parsed.custom_rules || [],
        custom_agents: parsed.custom_agents || [],
        user_agents: parsed.user_agents || [],
        custom_commands: parsed.custom_commands || [],
        user_commands: parsed.user_commands || [],
        tools: parsed.tools || [],
      };
      setSelectedName(name);
      setIsCreating(false);
      setProject(data);
      // Keep the overview card in sync whenever a project is reloaded from disk.
      setProjectDetailsMap((prev) => new Map(prev).set(name, data));
      setDirty(false);

      await loadAvailableSkills();
      await loadAvailableMcpServers();
      await loadMemories(name);
      await loadGroups(name);
      await loadActivity(name);
      await loadContext(name);
      await loadBuildItems(name);
      notifyProjectUpdated();
      // Reset activity tab pagination on project reload
      setActivityPage(0);
      setActivityPageEntries([]);
      setActivityTotalCount(0);

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

      // In the wizard (isCreating), selectedProjectTemplates represents the final
      // template choices from step 3. Merge them into the project snapshot here so
      // the save is atomic — no React state-update timing issues.
      let effectiveProject = project;
      if (isCreating && selectedProjectTemplates.length > 0) {
        const wizardTemplates = availableProjectTemplates.filter((t) =>
          selectedProjectTemplates.includes(t.name)
        );
        let mergedAgents = [...project.agents];
        let mergedSkills = [...project.skills];
        let mergedMcpServers = [...project.mcp_servers];
        let mergedProviders = [...project.providers];
        let anyUnified = false;
        const wizardPending: { content: string; rules: string[] }[] = [];

        for (const tmpl of wizardTemplates) {
          mergedAgents = [...new Set([...mergedAgents, ...tmpl.agents])];
          mergedSkills = [...new Set([...mergedSkills, ...tmpl.skills])];
          mergedMcpServers = [...new Set([...mergedMcpServers, ...tmpl.mcp_servers])];
          mergedProviders = [...new Set([...mergedProviders, ...tmpl.providers])];
          const hasContent = !!(tmpl.unified_instruction && tmpl.unified_instruction.trim());
          const hasRules = (tmpl.unified_rules || []).length > 0;
          if (hasContent || hasRules) {
            anyUnified = true;
            wizardPending.push({ content: tmpl.unified_instruction || "", rules: tmpl.unified_rules || [] });
          }
        }
        effectiveProject = {
          ...project,
          agents: mergedAgents,
          skills: mergedSkills,
          mcp_servers: mergedMcpServers,
          providers: mergedProviders,
          ...(anyUnified ? { instruction_mode: "unified" } : {}),
        };
        if (wizardPending.length > 0) {
          // Merge with any previously stashed pending entries (e.g. from startCreate)
          pendingUnifiedInstruction.current = [
            ...(pendingUnifiedInstruction.current ?? []),
            ...wizardPending,
          ];
        }
      }

      const toSave = { ...effectiveProject, name, updated_at: new Date().toISOString() };
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
        // Clear the stub reference so the unmount cleanup does not delete the
        // project we just successfully saved.
        wizardStubName.current = null;
        setIsCreating(false);
        await loadProjects();
      } else {
        trackProjectUpdated(name, {
          agent_count: toSave.agents.length,
          skill_count: toSave.skills.length,
          mcp_count: (toSave.mcp_servers ?? []).length,
        });
        // Keep the overview card in sync with what was just persisted.
        setProjectDetailsMap((prev) => new Map(prev).set(name, toSave));
      }
      setError(null);

      setSyncStatus(toSave.directory && toSave.agents.length > 0
        ? "Saved & synced"
        : "Saved");
      if (toSave.directory && toSave.agents.length > 0) {
        setDriftReport({ drifted: false, agents: [] });
        setDriftByProject((prev) => ({ ...prev, [name]: false }));
      }

      // Write any pending unified instruction content from one or more template applies.
      // Multiple entries are concatenated with a separator; rules are unioned across all.
      const pending = pendingUnifiedInstruction.current;
      if (pending !== null && pending.length > 0 && toSave.directory && toSave.agents.length > 0) {
        pendingUnifiedInstruction.current = null;
        const mergedRules = [...new Set(pending.flatMap((e) => e.rules))];
        const mergedContent = pending
          .map((e) => e.content)
          .filter(Boolean)
          .join("\n\n---\n\n");
        // If any template had rules, persist them into file_rules._project before writing.
        // Using _project (not _unified) ensures template rules are visible in the Rules tab
        // and are not silently dropped when the user later toggles rules from the Rules UI
        // (which only reads/writes _project).
        if (mergedRules.length > 0) {
          const latestRaw: string = await invoke("read_project", { name });
          const latestProj = JSON.parse(latestRaw);
          const existingProjectRules: string[] = (latestProj.file_rules || {})["_project"] || [];
          const combinedRules = [...new Set([...existingProjectRules, ...mergedRules])];
          const withRules = {
            ...latestProj,
            file_rules: { ...(latestProj.file_rules || {}), _project: combinedRules },
          };
          await invoke("save_project", { name, data: JSON.stringify(withRules, null, 2) });
        }
        await invoke("save_project_file", {
          name,
          filename: "_unified",
          content: mergedContent,
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
      // Clean up folder membership for deleted project
      saveFolders(folders.map((f) => ({ ...f, projectNames: f.projectNames.filter((n) => n !== name) })));
      await loadProjects();
      setError(null);
    } catch (err: any) {
      setError(`Failed to remove project: ${err}`);
    }
  };

  const startCreate = async (opts?: { fromTemplates?: ProjectTemplate[] }) => {
    setSelectedName(null);
    localStorage.removeItem(LAST_PROJECT_KEY);
    if (!opts?.fromTemplates?.length) setWizardSourceTemplates([]);
    // Pre-populate agents and agent options from settings defaults
    let defaultAgents: string[] = [];
    let defaultAgentOptions: Record<string, AgentOptions> = {};
    try {
      const raw: any = await invoke("read_settings");
      defaultAgents = raw.default_agents ?? [];
      defaultAgentOptions = raw.default_agent_options ?? {};
    } catch {
      // Non-fatal — proceed with empty agents if settings can't be read
    }

    // If launched from one or more templates, merge all their values into the initial project state.
    const templates = opts?.fromTemplates ?? [];
    const baseProject = {
      ...emptyProject(""),
      agents: defaultAgents,
      ...(Object.keys(defaultAgentOptions).length > 0
        ? { agent_options: defaultAgentOptions }
        : {}),
    };

    let mergedAgents = [...defaultAgents];
    let mergedSkills: string[] = [];
    let mergedMcpServers: string[] = [];
    let mergedProviders: string[] = [];
    let mergedDescription = "";
    let anyUnified = false;
    const pendingEntries: { content: string; rules: string[] }[] = [];

    for (const tmpl of templates) {
      mergedAgents = [...new Set([...mergedAgents, ...tmpl.agents])];
      mergedSkills = [...new Set([...mergedSkills, ...tmpl.skills])];
      mergedMcpServers = [...new Set([...mergedMcpServers, ...tmpl.mcp_servers])];
      mergedProviders = [...new Set([...mergedProviders, ...tmpl.providers])];
      if (!mergedDescription) mergedDescription = tmpl.description || "";
      const hasContent = !!(tmpl.unified_instruction && tmpl.unified_instruction.trim());
      const hasRules = (tmpl.unified_rules || []).length > 0;
      if (hasContent || hasRules) {
        anyUnified = true;
        pendingEntries.push({ content: tmpl.unified_instruction || "", rules: tmpl.unified_rules || [] });
      }
    }

    const initialProject = templates.length > 0
      ? {
          ...baseProject,
          description: mergedDescription,
          agents: mergedAgents,
          skills: mergedSkills,
          mcp_servers: mergedMcpServers,
          providers: mergedProviders,
          ...(anyUnified ? { instruction_mode: "unified" as const } : {}),
        }
      : baseProject;

    if (pendingEntries.length > 0) {
      pendingUnifiedInstruction.current = pendingEntries;
    }

    setProject(initialProject);
    setDirty(true);
    setIsCreating(true);
    setNewName("");
    setSelectedProjectTemplates(templates.map((t) => t.name));
    setShowProjectTemplatePicker(false);
    setWizardStep(1);
    setWizardDiscoveredAgents([]);
    setWizardDiscovering(false);
    wizardStubName.current = null;
  };

  /**
   * Cancel an in-progress project creation wizard.
   * If a stub was already saved to disk (after step 1 "Continue"), delete it so
   * it does not appear as a broken project in the project list.
   */
  const cancelCreate = async () => {
    const stub = wizardStubName.current;
    wizardStubName.current = null;
    setIsCreating(false);
    setProject(null);
    setDirty(false);
    setError(null);
    pendingUnifiedInstruction.current = null;
    if (stub) {
      try {
        await invoke("delete_project", { name: stub });
      } catch {
        // Non-fatal — stub cleanup is best-effort
      }
      await loadProjects();
    }
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

  // Persist a project snapshot directly — used by addItem/removeItem so they
  // can pass the already-computed new value without waiting for a React state flush.
  const saveProjectSnapshot = async (snapshot: Project): Promise<boolean> => {
    const folderFallback = snapshot.directory?.split("/").filter(Boolean).pop() ?? "";
    const name = isCreating ? (newName.trim() || folderFallback) : selectedName;
    if (!name) return false;
    try {
      const toSave = { ...snapshot, name, updated_at: new Date().toISOString() };
      await invoke("save_project", { name, data: JSON.stringify(toSave, null, 2) });
      setSyncStatus(toSave.directory && toSave.agents.length > 0 ? "Saved & synced" : "Saved");
      setProjectDetailsMap((prev) => new Map(prev).set(name, toSave));
      setDirty(false);
      return true;
    } catch (err: any) {
      console.error("Autosave failed:", err);
      setSyncStatus(`Save failed: ${err}`);
      return false;
    }
  };

  const addItem = async (key: ListField, item: string): Promise<boolean> => {
    if (!project || !item.trim()) return false;
    if (project[key].includes(item.trim())) return true;
    const newList = [...project[key], item.trim()];
    updateField(key, newList);
    const pName = isCreating ? newName.trim() : (selectedName ?? "");
    if (key === "agents") trackProjectAgentAdded(pName, item.trim());
    else if (key === "skills") {
      trackProjectSkillAdded(pName, item.trim());
      return await saveProjectSnapshot({ ...project, skills: newList as string[] });
    } else if (key === "mcp_servers") {
      trackProjectMcpServerAdded(pName, item.trim());
      const nextProject = {
        ...project,
        mcp_servers: newList as string[],
        disabled_mcp_servers: (project.disabled_mcp_servers || []).filter((name) => name !== item.trim()),
      };
      setProject(nextProject);
      return await saveProjectSnapshot(nextProject);
    }
    return true;
  };

  const removeItem = (key: ListField, idx: number) => {
    if (!project) return;
    const removed = project[key][idx];
    const newList = project[key].filter((_, i) => i !== idx);
    updateField(key, newList);
    const pName = isCreating ? newName.trim() : (selectedName ?? "");
    if (removed) {
      if (key === "agents") trackProjectAgentRemoved(pName, removed);
      else if (key === "skills") {
        trackProjectSkillRemoved(pName, removed);
        saveProjectSnapshot({ ...project, skills: newList as string[] });
      } else if (key === "mcp_servers") {
        trackProjectMcpServerRemoved(pName, removed);
        const nextProject = {
          ...project,
          mcp_servers: newList as string[],
          disabled_mcp_servers: (project.disabled_mcp_servers || []).filter((name) => name !== removed),
        };
        setProject(nextProject);
        saveProjectSnapshot(nextProject);
      }
    }
  };

  const isMcpServerEnabled = (server: string): boolean => {
    if (!project || server === "automatic") return true;
    return !(project.disabled_mcp_servers || []).includes(server);
  };

  const toggleMcpServerEnabled = async (server: string, enabled: boolean) => {
    if (!project || server === "automatic") return;
    const disabledServers = project.disabled_mcp_servers || [];
    const nextDisabledServers = enabled
      ? disabledServers.filter((name) => name !== server)
      : [...new Set([...disabledServers, server])];

    const nextProject = { ...project, disabled_mcp_servers: nextDisabledServers };
    setProject(nextProject);
    setDirty(true);
    await saveProjectSnapshot(nextProject);
  };

  const handleDismissRecommendation = async (id: number) => {
    try {
      await invoke("dismiss_recommendation", { id });
      setRecommendations((prev) => prev.filter((r) => r.id !== id));
    } catch (err: any) {
      console.error("Failed to dismiss recommendation:", err);
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

  // ── Instruction file conflict resolution ──────────────────────────────────

  /** User chose "Use existing file" — adopt the on-disk content into the editor. */
  const handleAdoptInstructionFile = async (filename: string, adoptedContent: string) => {
    const name = selectedName;
    if (!name) return;
    try {
      await invoke("adopt_instruction_file", { name, filename });
      // Update the editor state so it reflects the adopted content.
      if (activeProjectFile === filename || activeProjectFile === "_unified") {
        setProjectFileContent(adoptedContent);
        setProjectFileDirty(false);
      }
      // Re-run drift check: conflict should now be gone.
      const raw: string = await invoke("check_project_drift", { name });
      const report = JSON.parse(raw) as DriftReport;
      setDriftReport(report);
      setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
      notifyProjectUpdated();
    } catch (err: any) {
      setError(`Failed to adopt instruction file: ${err}`);
    } finally {
      setInstructionConflict(null);
    }
  };

  /** User chose "Overwrite with Automatic content" — wipe the externally-added content. */
  const handleOverwriteInstructionFile = async (filename: string) => {
    const name = selectedName;
    if (!name) return;
    try {
      await invoke("overwrite_instruction_file", { name, filename });
      // Clear the editor content to reflect the overwrite.
      if (activeProjectFile === filename || activeProjectFile === "_unified") {
        setProjectFileContent("");
        setProjectFileDirty(false);
      }
      // Re-run drift check.
      const raw: string = await invoke("check_project_drift", { name });
      const report = JSON.parse(raw) as DriftReport;
      setDriftReport(report);
      setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
      notifyProjectUpdated();
    } catch (err: any) {
      setError(`Failed to overwrite instruction file: ${err}`);
    } finally {
      setInstructionConflict(null);
    }
  };

  /** Re-check drift after a stale skill was adopted, removed, or overwritten. */
  const handleDriftResolved = async () => {
    const name = selectedName;
    if (!name) return;
    try {
      const raw: string = await invoke("check_project_drift", { name });
      const report = JSON.parse(raw) as DriftReport;
      setDriftReport(report);
      setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
      notifyProjectUpdated();
      // Re-read the project to pick up config changes (e.g. adopted skill).
      const projRaw: string = await invoke("read_project", { name });
      setProject(JSON.parse(projRaw));
    } catch {
      // Silently ignore — next periodic check will catch up.
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
      notifyProjectUpdated();
    } catch (err: any) {
      setSyncStatus(`Sync failed: ${err}`);
    }

    setTimeout(() => setSyncStatus(null), 4000);
  };

  const handleRebuild = async () => {
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;

    try {
      setSyncStatus("Preparing rebuild...");
      const rawPreview = await invoke<RebuildPreview | string>("preview_rebuild_project", { name });
      const preview = parseInvokeResult<RebuildPreview>(rawPreview);
      setRebuildPreview(preview);
      setSyncStatus(null);
    } catch (err: any) {
      setSyncStatus(`Rebuild failed: ${err}`);
      setTimeout(() => setSyncStatus(null), 4000);
    }
  };

  const confirmRebuild = async () => {
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;

    try {
      setRebuildBusy(true);
      setSyncStatus("Rebuilding...");
      await invoke("rebuild_project", { name });
      await reloadProject(name);
      setDirty(false);
      setDriftReport({ drifted: false, agents: [] });
      setDriftByProject((prev) => ({ ...prev, [name]: false }));
      notifyProjectUpdated();
      setRebuildPreview(null);
      setSyncStatus("Rebuilt project state");
    } catch (err: any) {
      setSyncStatus(`Rebuild failed: ${err}`);
    } finally {
      setRebuildBusy(false);
    }

    setTimeout(() => setSyncStatus(null), 4000);
  };

  const handleSyncAll = async () => {
    // Collect all projects currently showing drift
    const driftedProjects = projects.filter((n) => driftByProject[n] === true);
    if (driftedProjects.length === 0) return;

    setSyncAllStatus("syncing");
    try {
      // Sync each drifted project sequentially so we don't flood the backend
      for (const name of driftedProjects) {
        try {
          const result: string = await invoke("sync_project", { name });
          const files: string[] = JSON.parse(result);
          trackProjectSynced(name);
          // Mark this project as clean immediately so the UI reflects progress
          setDriftByProject((prev) => ({ ...prev, [name]: false }));
          // Satisfy the 'files' variable (used for analytics / future use)
          void files;
        } catch (_err) {
          // Continue with the next project even if one fails
        }
      }
      notifyProjectUpdated();
    } finally {
      setSyncAllStatus("idle");
    }
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

  // Deselect: return to overview grid
  const handleBackToOverview = () => {
    setSelectedName(null);
    localStorage.removeItem(LAST_PROJECT_KEY);
    setProject(null);
    setDirty(false);
    setIsCreating(false);
  };

  // Show the full-width card grid when nothing is selected and we are not creating
  if (!selectedName && !isCreating) {
    return (
      <>
        <div className="flex h-full w-full bg-bg-base relative overflow-hidden">
          {/* Collapsed trigger strip */}
          {!sidebarOpen && (
            <div
              ref={sidebarTriggerRef}
              className="flex-shrink-0 flex flex-col items-center border-r border-border-strong/40 bg-bg-input/50 relative z-30 cursor-pointer select-none"
              style={{ width: 28 }}
              onClick={toggleSidebarOpen}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  openSidebar();
                }
              }}
              role="button"
              tabIndex={0}
              title="Show projects"
            >
              <div className="flex flex-col items-center justify-center h-full gap-1 text-text-muted hover:text-text-base transition-colors">
                <ChevronRight size={12} />
                <span
                  className="text-[10px] font-semibold tracking-widest uppercase"
                  style={{ writingMode: "vertical-rl", transform: "rotate(180deg)" }}
                >
                  Projects
                </span>
              </div>
            </div>
          )}

          {/* Flyout / pinned sidebar panel */}
          <div
            ref={sidebarPanelRef}
            className={`flex flex-col border-r border-border-strong/40 bg-bg-input/50 relative transition-all duration-200 ${
              sidebarOpen
                ? sidebarPinned
                  ? "flex-shrink-0"
                  : "absolute left-0 top-0 h-full z-40 shadow-xl"
                : "hidden"
            }`}
            style={{ width: sidebarWidth }}
          >
            <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center bg-bg-base/30">
              <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                Projects
              </span>
              <div className="flex items-center gap-0.5">
                {!sidebarPinned && (
                  <button
                    onClick={closeSidebar}
                    className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
                    title="Close project tray"
                  >
                    <X size={13} />
                  </button>
                )}
                <button
                  onClick={toggleSidebarPin}
                  className={`transition-colors p-1 hover:bg-bg-sidebar rounded ${sidebarPinned ? "text-brand" : "text-text-muted hover:text-text-base"}`}
                  title={sidebarPinned ? "Unpin sidebar" : "Pin sidebar open"}
                >
                  {sidebarPinned ? <Pin size={13} /> : <PinOff size={13} />}
                </button>
                <button
                  onClick={() => createFolder()}
                  className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
                  title="New Folder"
                >
                  <FolderPlus size={14} />
                </button>
                <button
                  onClick={() => startCreate()}
                  className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
                  title="Create New Project"
                >
                  <Plus size={14} />
                </button>
              </div>
            </div>

            <div ref={sidebarListRef} data-ungrouped-zone="" className="flex-1 overflow-y-auto py-2 custom-scrollbar">
              {projects.length === 0 && folders.length === 0 ? (
                <div className="px-4 py-3 text-[13px] text-text-muted text-center">
                  No projects yet.
                </div>
              ) : (
                <ul className="space-y-0.5 px-2" ref={listRef} data-ungrouped-zone="">
                  {/* ── Folders ─────────────────────────────────────────── */}
                  {folders.map((folder, folderIdx) => (
                    <li key={folder.id} data-folder-reorder-idx={folderIdx} className="relative">
                      {draggingFolderId && folderReorderDropIdx === folderIdx && folderReorderDropIdx !== folders.findIndex((f) => f.id === draggingFolderId) && folderReorderDropIdx !== folders.findIndex((f) => f.id === draggingFolderId) + 1 && (
                        <div className="absolute -top-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                      )}
                      <div
                        data-folder-id={folder.id}
                        className={`group relative flex items-center gap-1.5 px-2 py-1 rounded-md transition-colors cursor-pointer ${
                          draggingFolderId === folder.id
                            ? "opacity-30"
                            : dragOverFolderId === folder.id
                            ? "bg-brand/15 ring-1 ring-brand/40"
                            : selectedFolderId === folder.id
                            ? "bg-bg-sidebar text-text-base"
                            : "hover:bg-bg-sidebar/40"
                        }`}
                      >
                        <div
                          className="absolute left-0 top-0 bottom-0 flex items-center pl-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing transition-opacity touch-none select-none z-10"
                          onPointerDown={(e) => handleFolderGripDown(folder.id, e)}
                        >
                          <GripVertical size={10} className="text-text-muted" />
                        </div>
                        {/* Chevron toggles collapse; folder name/icon sets the overview filter */}
                        <button
                          onClick={(e) => { e.stopPropagation(); if (!draggingFolderId) toggleFolderCollapsed(folder.id); }}
                          className="flex-shrink-0 p-0.5 text-text-muted hover:text-text-base"
                          title={folder.collapsed ? "Expand folder" : "Collapse folder"}
                        >
                          <ChevronDown
                            size={10}
                            className={`flex-shrink-0 transition-transform ${folder.collapsed ? "-rotate-90" : ""}`}
                          />
                        </button>
                        <button
                          onClick={() => { if (!draggingFolderId) setSelectedFolderId((prev) => prev === folder.id ? null : folder.id); }}
                          className="flex items-center gap-1.5 flex-1 min-w-0 text-left"
                          title={`Filter to ${folder.name}`}
                        >
                          <Folder
                            size={13}
                            className={`flex-shrink-0 ${selectedFolderId === folder.id ? "text-brand" : "text-text-muted"}`}
                          />
                          {editingFolderId === folder.id ? (
                            <input
                              type="text"
                              value={editingFolderName}
                              autoFocus
                              className="flex-1 min-w-0 bg-transparent text-[12px] font-medium text-text-base outline-none border-b border-brand"
                              onChange={(e) => setEditingFolderName(e.target.value)}
                              onBlur={() => {
                                const trimmed = editingFolderName.trim();
                                if (trimmed) renameFolder(folder.id, trimmed);
                                setEditingFolderId(null);
                              }}
                              onKeyDown={(e) => {
                                if (e.key === "Enter") {
                                  const trimmed = editingFolderName.trim();
                                  if (trimmed) renameFolder(folder.id, trimmed);
                                  setEditingFolderId(null);
                                } else if (e.key === "Escape") {
                                  setEditingFolderId(null);
                                }
                              }}
                              onClick={(e) => e.stopPropagation()}
                            />
                          ) : (
                            <span className={`flex-1 text-[12px] font-medium truncate ${selectedFolderId === folder.id ? "text-text-base" : "text-text-muted"}`}>
                              {folder.name}
                            </span>
                          )}
                          <span className="text-[10px] text-text-muted/60 flex-shrink-0 ml-1">
                            {folder.projectNames.filter((n) => projects.includes(n)).length}
                          </span>
                        </button>
                        <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              setEditingFolderId(folder.id);
                              setEditingFolderName(folder.name);
                            }}
                            className="p-0.5 text-text-muted hover:text-text-base rounded hover:bg-bg-sidebar"
                            title="Rename folder"
                          >
                            <Edit2 size={10} />
                          </button>
                          <button
                            onClick={(e) => { e.stopPropagation(); deleteFolder(folder.id); }}
                            className="p-0.5 text-text-muted hover:text-danger rounded hover:bg-bg-sidebar"
                            title="Delete folder (projects remain)"
                          >
                            <X size={10} />
                          </button>
                        </div>
                      </div>

                      {!folder.collapsed && (
                        <ul className="mt-0.5 space-y-0.5">
                          {folder.projectNames
                            .filter((n) => projects.includes(n))
                            .map((name, itemIdx) => (
                              <li
                                key={name}
                                className="relative pl-4"
                                data-folder-item-fid={folder.id}
                                data-folder-item-idx={itemIdx}
                              >
                                {folderDropTarget && folderDropTarget.folderId === folder.id && folderDropTarget.itemIdx === itemIdx && (
                                  <div className="absolute -top-[1px] left-6 right-2 h-[2px] bg-brand rounded-full z-10" />
                                )}
                                <div className={`group flex items-center relative ${dragGhost?.name === name ? "opacity-30" : ""}`}>
                                  <div
                                    className="absolute left-0 top-0 bottom-0 flex items-center pl-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing transition-opacity touch-none select-none z-10"
                                    onPointerDown={(e) => handleGripDown(name, folder.id, e)}
                                  >
                                    <GripVertical size={10} className="text-text-muted" />
                                  </div>
                                  <button
                                    onClick={() => { if (!dragGhost) selectProject(name); }}
                                    className="w-full flex items-center gap-2 pl-4 pr-2 py-1.5 rounded-md text-[13px] font-medium transition-colors text-text-muted hover:bg-bg-sidebar/50 hover:text-text-base"
                                  >
                                    <FolderOpen
                                      size={13}
                                      className={driftByProject[name] === true ? "text-warning" : "text-text-muted"}
                                    />
                                    <span className="flex-1 text-left truncate">{name}</span>
                                  </button>
                                  <button
                                    onClick={(e) => { e.stopPropagation(); moveProjectToFolder(name, null); }}
                                    className="absolute right-2 p-1 text-text-muted hover:text-text-base opacity-0 group-hover:opacity-100 hover:bg-surface rounded transition-all"
                                    title="Remove from folder"
                                  >
                                    <X size={10} />
                                  </button>
                                </div>
                                {folderDropTarget && folderDropTarget.folderId === folder.id && folderDropTarget.itemIdx === itemIdx + 1 && itemIdx === folder.projectNames.filter((n) => projects.includes(n)).length - 1 && (
                                  <div className="absolute -bottom-[1px] left-6 right-2 h-[2px] bg-brand rounded-full z-10" />
                                )}
                              </li>
                            ))}
                          {folder.projectNames.filter((n) => projects.includes(n)).length === 0 && (
                            <li className="pl-8 py-1 text-[11px] text-text-muted/50 italic">
                              Empty — drag projects here
                            </li>
                          )}
                        </ul>
                      )}
                      {draggingFolderId && folderReorderDropIdx === folderIdx + 1 && folderIdx === folders.length - 1 && folderReorderDropIdx !== folders.findIndex((f) => f.id === draggingFolderId) && (
                        <div className="absolute -bottom-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                      )}
                    </li>
                  ))}

                  {/* ── Ungrouped projects ───────────────────────────────── */}
                  {ungroupedProjects.map((name, idx) => (
                    <li
                      key={name}
                      className="relative"
                      data-ungrouped-idx={idx}
                    >
                      {dragGhost !== null && dropIdx === idx && (dragIdx === null || (dropIdx !== dragIdx && dropIdx !== dragIdx + 1)) && (
                        <div className="absolute -top-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                      )}
                      <div className={`group flex items-center relative ${dragGhost?.name === name ? "opacity-30" : ""}`}>
                        <div
                          className="absolute left-0 top-0 bottom-0 flex items-center pl-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing transition-opacity touch-none select-none z-10"
                          onPointerDown={(e) => handleGripDown(name, null, e)}
                        >
                          <GripVertical size={10} className="text-text-muted" />
                        </div>
                        <button
                          onClick={() => { if (!dragGhost) selectProject(name); }}
                          className="w-full flex items-center gap-2.5 pl-4 pr-2 py-1.5 rounded-md text-[13px] font-medium transition-colors text-text-muted hover:bg-bg-sidebar/50 hover:text-text-base"
                        >
                          <FolderOpen
                            size={14}
                            className={driftByProject[name] === true ? "text-warning" : "text-text-muted"}
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
                      {dragGhost !== null && dropIdx === ungroupedProjects.length && idx === ungroupedProjects.length - 1 && (dragIdx === null || dropIdx !== dragIdx) && (
                        <div className="absolute -bottom-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </div>
            {/* Resize handle (only when pinned) */}
            {sidebarPinned && (
              <div
                className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-brand/40 active:bg-brand/60 transition-colors z-10"
                onMouseDown={onSidebarMouseDown}
              />
            )}
          </div>

          {/* Right area - overview card grid */}
          <div className="flex-1 min-w-0 h-full overflow-hidden">
            <ProjectsOverview
              projects={projects}
              projectsLoading={projectsLoading}
              projectDetails={projectDetailsMap}
              driftByProject={driftByProject}
              folders={folders}
              onSelect={(name) => {
                setSelectedName(name);
                selectProject(name);
              }}
              onCreate={() => startCreate()}
              onSyncAll={handleSyncAll}
              syncAllStatus={syncAllStatus}
              selectedFolder={selectedFolderId ? (folders.find((f) => f.id === selectedFolderId) ?? null) : null}
              onClearFolder={() => setSelectedFolderId(null)}
            />
          </div>
        </div>
        {driftDiffFile && (
          <DriftDiffModal
            file={driftDiffFile.file}
            agentLabel={driftDiffFile.agentLabel}
            projectName={selectedName ?? undefined}
            onClose={() => setDriftDiffFile(null)}
            onResolved={handleDriftResolved}
          />
        )}
        {instructionConflict && selectedName && (
          <InstructionConflictModal
            conflict={instructionConflict}
            projectName={selectedName}
            onAdopt={(adopted) => handleAdoptInstructionFile(instructionConflict.filename, adopted)}
            onOverwrite={() => handleOverwriteInstructionFile(instructionConflict.filename)}
            onClose={() => setInstructionConflict(null)}
          />
        )}
      </>
    );
  }

  return (
    <>
    <div className="flex h-full w-full bg-bg-base relative overflow-hidden">
      {/* Left sidebar - project list (hidden while creating a new project) */}
      {/* Collapsed trigger strip */}
      {!isCreating && !sidebarOpen && (
        <div
          ref={sidebarTriggerRef}
          className="flex-shrink-0 flex flex-col items-center border-r border-border-strong/40 bg-bg-input/50 relative z-30 cursor-pointer select-none"
          style={{ width: 28 }}
          onClick={toggleSidebarOpen}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              openSidebar();
            }
          }}
          role="button"
          tabIndex={0}
          title="Show projects"
        >
          <div className="flex flex-col items-center justify-center h-full gap-1 text-text-muted hover:text-text-base transition-colors">
            <ChevronRight size={12} />
            <span
              className="text-[10px] font-semibold tracking-widest uppercase"
              style={{ writingMode: "vertical-rl", transform: "rotate(180deg)" }}
            >
              Projects
            </span>
          </div>
        </div>
      )}

      {/* Flyout / pinned sidebar panel */}
      {!isCreating && (
        <div
          ref={sidebarPanelRef}
          className={`flex flex-col border-r border-border-strong/40 bg-bg-input/50 relative transition-all duration-200 ${
            sidebarOpen
              ? sidebarPinned
                ? "flex-shrink-0"
                : "absolute left-0 top-0 h-full z-40 shadow-xl"
              : "hidden"
          }`}
          style={{ width: sidebarWidth }}
        >
        <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center bg-bg-base/30">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Projects
          </span>
          <div className="flex items-center gap-0.5">
            {!sidebarPinned && (
              <button
                onClick={closeSidebar}
                className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
                title="Close project tray"
              >
                <X size={13} />
              </button>
            )}
            <button
              onClick={toggleSidebarPin}
              className={`transition-colors p-1 hover:bg-bg-sidebar rounded ${sidebarPinned ? "text-brand" : "text-text-muted hover:text-text-base"}`}
              title={sidebarPinned ? "Unpin sidebar" : "Pin sidebar open"}
            >
              {sidebarPinned ? <Pin size={13} /> : <PinOff size={13} />}
            </button>
            <button
              onClick={() => createFolder()}
              className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
              title="New Folder"
            >
              <FolderPlus size={14} />
            </button>
            <button
              onClick={() => startCreate()}
              className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
              title="Create New Project"
            >
              <Plus size={14} />
            </button>
          </div>
        </div>

        <div ref={sidebarListRef} data-ungrouped-zone="" className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {projects.length === 0 && !isCreating && folders.length === 0 ? (
            <div className="px-4 py-3 text-[13px] text-text-muted text-center">
              No projects yet.
            </div>
          ) : (
            <ul className="space-y-0.5 px-2" ref={listRef} data-ungrouped-zone="">


              {/* ── Folders ─────────────────────────────────────────── */}
              {folders.map((folder, folderIdx) => (
                <li key={folder.id} data-folder-reorder-idx={folderIdx} className="relative">
                  {/* Drop indicator line — above this folder (for folder reordering) */}
                  {draggingFolderId && folderReorderDropIdx === folderIdx && folderReorderDropIdx !== folders.findIndex((f) => f.id === draggingFolderId) && folderReorderDropIdx !== folders.findIndex((f) => f.id === draggingFolderId) + 1 && (
                    <div className="absolute -top-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                  )}
                  {/* Folder header */}
                  <div
                    data-folder-id={folder.id}
                    className={`group relative flex items-center gap-1.5 px-2 py-1 rounded-md transition-colors cursor-pointer ${
                      draggingFolderId === folder.id
                        ? "opacity-30"
                        : dragOverFolderId === folder.id
                        ? "bg-brand/15 ring-1 ring-brand/40"
                        : "hover:bg-bg-sidebar/40"
                    }`}
                  >
                    {/* Grip handle for folder reordering — absolutely positioned so it doesn't push content right */}
                    <div
                      className="absolute left-0 top-0 bottom-0 flex items-center pl-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing transition-opacity touch-none select-none z-10"
                      onPointerDown={(e) => handleFolderGripDown(folder.id, e)}
                    >
                      <GripVertical size={10} className="text-text-muted" />
                    </div>
                    <button
                      onClick={() => { if (!draggingFolderId) toggleFolderCollapsed(folder.id); }}
                      className="flex items-center gap-1.5 flex-1 min-w-0 text-left"
                    >
                      <ChevronDown
                        size={10}
                        className={`text-text-muted flex-shrink-0 transition-transform ${folder.collapsed ? "-rotate-90" : ""}`}
                      />
                      <Folder size={13} className="text-text-muted flex-shrink-0" />
                      {editingFolderId === folder.id ? (
                        <input
                          type="text"
                          value={editingFolderName}
                          autoFocus
                          className="flex-1 min-w-0 bg-transparent text-[12px] font-medium text-text-base outline-none border-b border-brand"
                          onChange={(e) => setEditingFolderName(e.target.value)}
                          onBlur={() => {
                            const trimmed = editingFolderName.trim();
                            if (trimmed) renameFolder(folder.id, trimmed);
                            setEditingFolderId(null);
                          }}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") {
                              const trimmed = editingFolderName.trim();
                              if (trimmed) renameFolder(folder.id, trimmed);
                              setEditingFolderId(null);
                            } else if (e.key === "Escape") {
                              setEditingFolderId(null);
                            }
                          }}
                          onClick={(e) => e.stopPropagation()}
                        />
                      ) : (
                        <span className="flex-1 text-[12px] font-medium text-text-muted truncate">
                          {folder.name}
                        </span>
                      )}
                      <span className="text-[10px] text-text-muted/60 flex-shrink-0 ml-1">
                        {folder.projectNames.filter((n) => projects.includes(n)).length}
                      </span>
                    </button>
                    {/* Folder actions (rename / delete) */}
                    <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setEditingFolderId(folder.id);
                          setEditingFolderName(folder.name);
                        }}
                        className="p-0.5 text-text-muted hover:text-text-base rounded hover:bg-bg-sidebar"
                        title="Rename folder"
                      >
                        <Edit2 size={10} />
                      </button>
                      <button
                        onClick={(e) => { e.stopPropagation(); deleteFolder(folder.id); }}
                        className="p-0.5 text-text-muted hover:text-danger rounded hover:bg-bg-sidebar"
                        title="Delete folder (projects remain)"
                      >
                        <X size={10} />
                      </button>
                    </div>
                  </div>

                  {/* Folder contents */}
                  {!folder.collapsed && (
                    <ul className="mt-0.5 space-y-0.5">
                      {folder.projectNames
                        .filter((n) => projects.includes(n))
                        .map((name, itemIdx) => (
                            <li
                              key={name}
                              className="relative pl-4"
                              data-folder-item-fid={folder.id}
                              data-folder-item-idx={itemIdx}
                            >
                              {/* Drop indicator line — above this folder item */}
                              {folderDropTarget && folderDropTarget.folderId === folder.id && folderDropTarget.itemIdx === itemIdx && (
                                <div className="absolute -top-[1px] left-6 right-2 h-[2px] bg-brand rounded-full z-10" />
                              )}
                              <div className={`group flex items-center relative ${dragGhost?.name === name ? "opacity-30" : ""}`}>
                                <div
                                  className="absolute left-0 top-0 bottom-0 flex items-center pl-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing transition-opacity touch-none select-none z-10"
                                  onPointerDown={(e) => handleGripDown(name, folder.id, e)}
                                >
                                  <GripVertical size={10} className="text-text-muted" />
                                </div>
                                <button
                                  onClick={() => { if (!dragGhost) selectProject(name); }}
                                  className={`w-full flex items-center gap-2 pl-4 pr-2 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
                                    selectedName === name && !isCreating
                                      ? "bg-bg-sidebar text-text-base"
                                      : "text-text-muted hover:bg-bg-sidebar/50 hover:text-text-base"
                                  }`}
                                >
                                  <FolderOpen
                                    size={13}
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
                                {/* Remove-from-folder button */}
                                <button
                                  onClick={(e) => { e.stopPropagation(); moveProjectToFolder(name, null); }}
                                  className="absolute right-2 p-1 text-text-muted hover:text-text-base opacity-0 group-hover:opacity-100 hover:bg-surface rounded transition-all"
                                  title="Remove from folder"
                                >
                                  <X size={10} />
                                </button>
                              </div>
                              {/* Drop indicator line — after last folder item */}
                              {folderDropTarget && folderDropTarget.folderId === folder.id && folderDropTarget.itemIdx === itemIdx + 1 && itemIdx === folder.projectNames.filter((n) => projects.includes(n)).length - 1 && (
                                <div className="absolute -bottom-[1px] left-6 right-2 h-[2px] bg-brand rounded-full z-10" />
                              )}
                            </li>
                        ))}
                      {folder.projectNames.filter((n) => projects.includes(n)).length === 0 && (
                        <li className="pl-8 py-1 text-[11px] text-text-muted/50 italic">
                          Empty — drag projects here
                        </li>
                      )}
                    </ul>
                  )}
                  {/* Drop indicator line — after last folder (for folder reordering) */}
                  {draggingFolderId && folderReorderDropIdx === folderIdx + 1 && folderIdx === folders.length - 1 && folderReorderDropIdx !== folders.findIndex((f) => f.id === draggingFolderId) && (
                    <div className="absolute -bottom-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                  )}
                </li>
              ))}

              {/* ── Ungrouped projects ───────────────────────────────── */}
              {ungroupedProjects.map((name, idx) => (
                <li
                  key={name}
                  className="relative"
                  data-ungrouped-idx={idx}
                >
                  {/* Drop indicator line — above this item */}
                  {dragGhost !== null && dropIdx === idx && (dragIdx === null || (dropIdx !== dragIdx && dropIdx !== dragIdx + 1)) && (
                    <div className="absolute -top-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                  )}
                  <div className={`group flex items-center relative ${dragGhost?.name === name ? "opacity-30" : ""}`}>
                    <div
                      className="absolute left-0 top-0 bottom-0 flex items-center pl-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing transition-opacity touch-none select-none z-10"
                      onPointerDown={(e) => handleGripDown(name, null, e)}
                    >
                      <GripVertical size={10} className="text-text-muted" />
                    </div>
                    <button
                      onClick={() => { if (!dragGhost) selectProject(name); }}
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
                  {/* Drop indicator line — after last ungrouped item */}
                  {dragGhost !== null && dropIdx === ungroupedProjects.length && idx === ungroupedProjects.length - 1 && (dragIdx === null || dropIdx !== dragIdx) && (
                    <div className="absolute -bottom-[1px] left-2 right-2 h-[2px] bg-brand rounded-full z-10" />
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
        {/* Resize handle (only when pinned — fly-out can't be resized) */}
        {sidebarPinned && (
          <div
            className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-brand/40 active:bg-brand/60 transition-colors z-10"
            onMouseDown={onSidebarMouseDown}
          />
        )}
      </div>
      )}

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
            {/* ── Top action bar: back + buttons ─────────────────── */}
            <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center flex-shrink-0">
              {/* Back to overview */}
              <button
                onClick={handleBackToOverview}
                className="flex items-center gap-1 text-text-muted hover:text-text-base transition-colors px-2 py-1 rounded hover:bg-bg-sidebar"
                title="Back to all projects"
              >
                <ChevronLeft size={14} />
                <span className="text-[12px]">Projects</span>
              </button>

              <div className="flex items-center gap-2">
                {syncStatus && (
                  <span className={`text-[12px] ${syncStatus.startsWith("Sync failed") ? "text-danger" : syncStatus === "syncing" ? "text-text-muted" : "text-success"}`}>
                    {syncStatus === "syncing" ? "Syncing..." : syncStatus}
                  </span>
                )}
                {/* Rebuild button */}
                {!isCreating && selectedName && (
                  <button
                    onClick={handleRebuild}
                    title="Rebuild Automatic state from current project files"
                    className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-surface-hover text-text-muted hover:text-text-base rounded text-[12px] font-medium border border-border-strong transition-colors shadow-sm"
                  >
                    <RotateCcw size={12} /> Rebuild
                  </button>
                )}
                {/* Apply Template button */}
                {!isCreating && selectedName && (
                  <button
                    onClick={() => setShowProjectTemplatePicker((v) => !v)}
                    title="Apply a project template"
                    className={`flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-brand/10 text-text-muted hover:text-brand rounded text-[12px] font-medium border border-border-strong hover:border-brand/40 transition-colors shadow-sm ${showProjectTemplatePicker ? "bg-brand/10 text-brand border-brand/40" : ""}`}
                  >
                    <LayoutTemplate size={12} /> Apply Template
                  </button>
                )}
                {/* Open in editor dropdown — only shown when a directory is set */}
                {!isCreating && project.directory && (
                  <div className="relative" ref={openInDropdownRef}>
                    <button
                      onClick={() => setOpenInDropdownOpen((v) => !v)}
                      className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-surface-hover text-text-base rounded text-[12px] font-medium border border-border-strong transition-colors shadow-sm"
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
                    className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-danger/10 text-text-base hover:text-danger rounded text-[12px] font-medium border border-border-strong hover:border-danger/40 transition-colors shadow-sm"
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
                      className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-warning/10 text-warning rounded text-[12px] font-medium border border-border-strong hover:border-warning/60 transition-colors shadow-sm"
                      title="Configuration has drifted — click to sync"
                    >
                      <RefreshCw size={12} /> Sync Configs
                    </button>
                  ) : driftReport && !driftReport.drifted ? (
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-success/10 text-success rounded text-[12px] font-medium border border-border-strong hover:border-success/40 transition-colors shadow-sm"
                      title="Configuration is up to date — click to force sync"
                    >
                      <Check size={12} /> In Sync
                    </button>
                  ) : (
                    /* driftReport === null: not yet checked */
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-surface-hover text-text-muted hover:text-text-base rounded text-[12px] font-medium border border-border-strong transition-colors shadow-sm"
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

            {/* ── Project title ───────────────────────────────────── */}
            {!isCreating && (
              <div className="px-6 pt-5 pb-4 border-b border-border-strong/40 flex-shrink-0 flex items-start justify-between gap-4">
                {/* Left: name + directory */}
                <div className="min-w-0 flex-1">
                  {isRenaming ? (
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
                      className="bg-transparent border-none outline-none text-[22px] font-semibold text-text-base placeholder-text-muted/50 w-full"
                    />
                  ) : (
                    <h1
                      className="text-[22px] font-semibold text-text-base cursor-text leading-tight"
                      onDoubleClick={startRename}
                      title="Double-click to rename"
                    >
                      {selectedName}
                    </h1>
                  )}
                  {/* Directory path — click to change */}
                  <button
                    onClick={async () => {
                      const selected: string | null = await invoke("open_directory_dialog");
                      if (selected) updateField("directory", selected);
                    }}
                    className="mt-1 flex items-center gap-1.5 text-[11px] text-text-muted hover:text-text-base font-mono transition-colors group"
                    title="Click to change directory"
                  >
                    <FolderOpen size={11} className="flex-shrink-0 text-text-muted/60 group-hover:text-text-muted transition-colors" />
                    {project.directory
                      ? <span className="truncate max-w-[480px]">{project.directory.replace(/^\/Users\/[^/]+/, "~")}</span>
                      : <span className="italic text-text-muted/50">No directory set — click to choose</span>
                    }
                  </button>
                </div>

                {/* Right: agent icons */}
                {project.agents.length > 0 && (
                  <button
                    onClick={() => selectTab("agents")}
                    className="flex items-center gap-1.5 flex-shrink-0 mt-1 group"
                    title="Agents — click to manage"
                  >
                    {project.agents.map((agentId) => (
                      <span
                        key={agentId}
                        className="opacity-70 group-hover:opacity-100 transition-opacity"
                        title={availableAgents.find(a => a.id === agentId)?.label ?? agentId}
                      >
                        <AgentIcon agentId={agentId} size={20} />
                      </span>
                    ))}
                  </button>
                )}
              </div>
            )}

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
                  {/* Instruction file conflicts within the drift banner */}
                  {(driftReport.instruction_conflicts ?? []).length > 0 && (
                    <div>
                      <div className="text-[11px] font-semibold text-warning/80 mb-0.5">Instruction files</div>
                      <div className="flex flex-wrap gap-x-2 gap-y-1">
                        {(driftReport.instruction_conflicts ?? []).map((c) => (
                          <button
                            key={c.filename}
                            onClick={() => setInstructionConflict(c)}
                            className="flex items-center gap-1 text-[11px] font-mono text-warning/70 hover:text-warning bg-warning/5 hover:bg-warning/15 border border-warning/20 hover:border-warning/40 rounded px-1.5 py-0.5 transition-colors"
                            title="Resolve conflict"
                          >
                            {c.filename}
                            <span className="text-warning/50 font-sans ml-0.5">(conflict)</span>
                          </button>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* ── Apply Template panel (shown from title bar button) ── */}
            {!isCreating && showProjectTemplatePicker && (
              <div className="border-b border-border-strong/40 bg-bg-input/50 px-6 py-3">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-[12px] font-semibold text-text-base">Apply Project Templates</span>
                  <button
                    onClick={() => setShowProjectTemplatePicker(false)}
                    className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                  >
                    <X size={13} />
                  </button>
                </div>
                {availableProjectTemplates.length === 0 ? (
                  <p className="text-[12px] text-text-muted py-2">
                    No project templates yet. Create one in the Project Templates section.
                  </p>
                ) : (
                  <>
                    <p className="text-[11px] text-text-muted mb-2">Select one or more templates — their assets will be merged into this project.</p>
                    <div className="grid grid-cols-2 xl:grid-cols-3 gap-2 mb-3">
                      {availableProjectTemplates.map((tmpl) => {
                        const isSelected = selectedProjectTemplates.includes(tmpl.name);
                        return (
                          <button
                            key={tmpl.name}
                            onClick={() => toggleProjectTemplateSelection(tmpl.name)}
                            className={`text-left px-3 py-2 rounded-md transition-colors flex items-start gap-2 border ${
                              isSelected ? "bg-brand/15 border-brand/40" : "bg-bg-sidebar hover:bg-surface border-border-strong/30 hover:border-border-strong"
                            }`}
                          >
                            <LayoutTemplate size={13} className={`mt-0.5 shrink-0 ${isSelected ? "text-brand" : "text-text-muted"}`} />
                            <div className="min-w-0">
                              <div className="text-[12px] font-medium text-text-base truncate">{tmpl.name}</div>
                              {tmpl.description && <div className="text-[11px] text-text-muted truncate">{tmpl.description}</div>}
                              <div className="flex items-center gap-2 mt-1">
                                {tmpl.agents.length > 0 && <span className="text-[10px] text-text-muted">{tmpl.agents.length} agents</span>}
                                {tmpl.skills.length > 0 && <span className="text-[10px] text-text-muted">{tmpl.skills.length} skills</span>}
                                {tmpl.mcp_servers.length > 0 && <span className="text-[10px] text-text-muted">{tmpl.mcp_servers.length} MCP</span>}
                              </div>
                            </div>
                            {isSelected && <Check size={12} className="text-brand shrink-0 mt-0.5 ml-auto" />}
                          </button>
                        );
                      })}
                    </div>
                    <div className="flex items-center justify-end gap-3">
                      {selectedProjectTemplates.length > 0 && (
                        <button
                          onClick={() => setSelectedProjectTemplates([])}
                          className="text-[12px] text-text-muted hover:text-text-base transition-colors"
                        >
                          Clear selection
                        </button>
                      )}
                      <button
                        onClick={applySelectedProjectTemplates}
                        disabled={selectedProjectTemplates.length === 0}
                        className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover disabled:opacity-40 disabled:cursor-not-allowed text-white text-[12px] font-medium rounded transition-colors"
                      >
                        <Check size={12} />
                        Apply {selectedProjectTemplates.length > 0 ? `${selectedProjectTemplates.length} ` : ""}Template{selectedProjectTemplates.length !== 1 ? "s" : ""}
                      </button>
                    </div>
                  </>
                )}
              </div>
            )}

            {/* ── New project wizard (3 steps) ─────────────────────── */}
            {isCreating && (
              <div className="flex-1 flex flex-col items-center justify-center p-8">
                <div className="w-full max-w-md relative">

                  {/* Cancel wizard button */}
                  <button
                    onClick={cancelCreate}
                    className="absolute -top-2 right-0 flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
                    title="Cancel project creation"
                  >
                    <X size={13} /> Cancel
                  </button>

                  {/* Template source badge */}
                  {wizardSourceTemplates.length > 0 && (
                    <div className="flex items-center justify-center gap-2 mb-5">
                      <div className="flex items-center gap-2 px-3 py-1.5 bg-brand/10 border border-brand/30 rounded-full">
                        <LayoutTemplate size={12} className="text-brand" />
                        <span className="text-[12px] text-brand font-medium">From template{wizardSourceTemplates.length > 1 ? "s" : ""}: {wizardSourceTemplates.join(", ")}</span>
                      </div>
                    </div>
                  )}

                  {/* Step indicator */}
                  <div className="flex items-center justify-center gap-2 mb-8">
                    {([1, 2, 3] as const).map((s) => (
                      <div key={s} className="flex items-center gap-2">
                        <div className={`w-6 h-6 rounded-full flex items-center justify-center text-[11px] font-semibold transition-colors ${
                          wizardStep === s
                            ? "bg-brand text-white"
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
                              const selected: string | null = await invoke("open_directory_dialog");
                              if (!selected) return;
                              const folderName = selected.split("/").filter(Boolean).pop() ?? "";
                              const name = newName.trim() || folderName;
                              setNewName(name);
                              updateField("directory", selected);
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
                                // Track stub name so cancelCreate can clean it up if the user navigates away
                                wizardStubName.current = name;
                                // Run read-only autodetection
                                const raw: string = await invoke("autodetect_project_dependencies", { name });
                                const detected = JSON.parse(raw) as Project;
                                // Merge: start from current project state (which holds any
                                // template-applied skills/MCP/agents), then add autodetected
                                // items on top. Use emptyProject only for structural defaults.
                                const currentProject = project ?? emptyProject(name);
                                const mergedAgents = [
                                  ...new Set([...currentProject.agents, ...detected.agents]),
                                ];
                                const mergedSkills = [
                                  ...new Set([...currentProject.skills, ...detected.skills]),
                                ];
                                const mergedMcp = [
                                  ...new Set([...currentProject.mcp_servers, ...detected.mcp_servers]),
                                ];
                                const mergedLocalSkills = [
                                  ...new Set([...(currentProject.local_skills ?? []), ...detected.local_skills]),
                                ];
                                setProject({
                                  ...currentProject,
                                  name,
                                  directory: dir,
                                  agents: mergedAgents,
                                  skills: mergedSkills,
                                  local_skills: mergedLocalSkills,
                                  mcp_servers: mergedMcp,
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

                  {/* ── Step 3: Templates ─────────────────────────────── */}
                  {wizardStep === 3 && (
                    <>
                      <div className="mb-6 text-center">
                        <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-brand/10 border border-brand/30 flex items-center justify-center">
                          <LayoutTemplate size={24} className="text-brand" strokeWidth={1.5} />
                        </div>
                        <h2 className="text-[16px] font-semibold text-text-base mb-1">Apply templates</h2>
                        <p className="text-[13px] text-text-muted leading-relaxed">
                          Optionally select one or more templates to pre-configure skills, MCP servers, and instructions.
                        </p>
                      </div>

                      {availableProjectTemplates.length > 0 ? (
                        <div className="space-y-1 max-h-56 overflow-y-auto custom-scrollbar mb-3">
                          {availableProjectTemplates.map((tmpl) => {
                            const isSelected = selectedProjectTemplates.includes(tmpl.name);
                            return (
                              <button
                                key={tmpl.name}
                                onClick={() => toggleProjectTemplateSelection(tmpl.name)}
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

                      {selectedProjectTemplates.length > 0 && (
                        <button
                          onClick={() => setSelectedProjectTemplates([])}
                          className="flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base mb-3 transition-colors"
                        >
                          <X size={11} /> Clear selection ({selectedProjectTemplates.length})
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
            {/* Primary group tabs */}
            <div className="flex items-center gap-0 px-6 border-b border-border-strong/40 flex-shrink-0">
              {PROJECT_GROUPS.map((group) => (
                <button
                  key={group.id}
                  onClick={() => selectGroup(group.id)}
                  className={`px-3 py-2.5 text-[13px] font-medium transition-colors relative flex items-center gap-1.5 ${
                    projectGroup === group.id
                      ? "text-text-base"
                      : "text-text-muted hover:text-text-base"
                  }`}
                >
                  {group.label}
                  {group.id === "insights" && recsDisplayCount > 0 && (
                    <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-warning/15 text-warning border border-warning/20 leading-none">
                      {recsDisplayCount}
                    </span>
                  )}
                  {projectGroup === group.id && (
                    <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-brand rounded-t" />
                  )}
                </button>
              ))}
            </div>
            {/* Secondary sub-tabs (only shown when a group with sub-tabs is active) */}
            {projectGroup !== "summary" && (() => {
              // Tools group: dynamic sub-tabs from loaded tool entries.
              if (projectGroup === "tools") {
                if (toolEntries.length === 0) return null;
                return (
                  <div className="flex items-center gap-0 px-6 border-b border-border-strong/20 bg-bg-input/30 flex-shrink-0">
                    {toolEntries.map((entry) => (
                      <button
                        key={entry.name}
                        onClick={() => setToolTab(entry.name)}
                        className={`px-3 py-2 text-[12px] font-medium transition-colors relative flex items-center gap-1.5 ${
                          toolTab === entry.name
                            ? "text-text-base"
                            : "text-text-muted hover:text-text-base"
                        }`}
                      >
                        {entry.display_name}
                        {toolTab === entry.name && (
                          <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-brand/60 rounded-t" />
                        )}
                      </button>
                    ))}
                  </div>
                );
              }
              // All other groups: static sub-tabs.
              const activeGroup = PROJECT_GROUPS.find((g) => g.id === projectGroup);
              if (!activeGroup || activeGroup.tabs.length <= 1) return null;
              return (
                <div className="flex items-center gap-0 px-6 border-b border-border-strong/20 bg-bg-input/30 flex-shrink-0">
                  {activeGroup.tabs.map((tab) => (
                    <button
                      key={tab.id}
                      onClick={() => selectTab(tab.id)}
                      className={`px-3 py-2 text-[12px] font-medium transition-colors relative flex items-center gap-1.5 ${
                        projectTab === tab.id
                          ? "text-text-base"
                          : "text-text-muted hover:text-text-base"
                      }`}
                    >
                      {tab.label}
                      {projectTab === tab.id && (
                        <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-brand/60 rounded-t" />
                      )}
                    </button>
                  ))}
                </div>
              );
            })()}

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
                              notifyProjectUpdated();
                            }
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                            (project.instruction_mode || "per-agent") === "unified"
                              ? "bg-brand text-white"
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
                              notifyProjectUpdated();
                            }
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                            (project.instruction_mode || "per-agent") === "per-agent"
                              ? "bg-brand text-white"
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
                                  className="w-full text-left px-2 py-1 text-[12px] bg-bg-sidebar hover:bg-brand text-text-base hover:text-white rounded transition-colors flex items-center gap-1.5"
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
                              {activeProjectFile === "_unified" ? "Shared File" : activeProjectFile}
                            </h3>
                            <p className="text-[13px] text-text-muted mb-5 max-w-xs">
                              This file doesn't exist yet. Create it to provide project instructions for {activeFile?.agents.join(" & ")}.
                            </p>
                             <div className="flex items-center gap-2">
                               {/* Primary action: Generate with AI */}
                                 <span className="relative group/keytip">
                                   <button
                                     onClick={handleGenerateInstruction}
                                     disabled={projectFileGenerating || !hasAnthropicKey}
                                     className="px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
                                   >
                                     <Sparkles size={12} className={projectFileGenerating ? "animate-pulse" : ""} />
                                     {projectFileGenerating ? "Generating…" : "Generate with AI"}
                                   </button>
                                   {!hasAnthropicKey && (
                                     <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                                       Add your Anthropic API key to access
                                     </span>
                                   )}
                                 </span>
                               {/* Secondary: blank file */}
                               <button
                                  onClick={() => {
                                    setProjectFileContent("");
                                    setProjectFileEditing(true);
                                    setProjectFileDirty(true);
                                  }}
                                  className="px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-border-strong/40 transition-colors flex items-center gap-1.5"
                                >
                                  <Plus size={12} /> Create File
                                </button>
                               {/* Secondary: from template */}
                               {availableTemplates.length > 0 && (
                                 <button
                                   onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                                   className="px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-border-strong/40 transition-colors flex items-center gap-1.5"
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
                                      className="px-2 py-1 text-[12px] bg-bg-sidebar hover:bg-brand text-text-base hover:text-white rounded transition-colors flex items-center gap-1.5"
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

                      return (
                        <div className="flex-1 flex min-w-0 min-h-0">
                          {/* Editor column */}
                          <div className="flex-1 flex flex-col min-w-0">
                             {/* Editor toolbar */}
                             <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
                               <div className="flex items-center gap-2 min-w-0">
                                 <span className="text-[11px] text-text-muted">
                                   {activeProjectFile === "_unified"
                                     ? <>{projectFileEditing ? "Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                                     : <>{activeProjectFile}{!fileExists ? " (new)" : ""}{projectFileEditing ? " — Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                                   }
                                 </span>
                                 <TokenPill text={projectFileContent} />
                               </div>
                                <div className="flex items-center gap-1.5">
                                   {/* Update with AI — only when content already exists */}
                                   {(fileExists || projectFileContent.trim().length > 0) && (
                                    <span className="relative group/keytip">
                                      <button
                                        onClick={handleUpdateInstruction}
                                        disabled={projectFileUpdating || projectFileGenerating || projectFileSaving || !hasAnthropicKey || !projectFileContent.trim()}
                                        className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                                      >
                                        <RefreshCw size={10} className={projectFileUpdating ? "animate-spin text-brand" : ""} />
                                        {projectFileUpdating ? "Updating…" : "Update"}
                                      </button>
                                      {!hasAnthropicKey && (
                                        <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                                          Add your Anthropic API key to access
                                        </span>
                                      )}
                                    </span>
                                   )}
                                   {/* Generate with AI — always visible */}
                                    <span className="relative group/keytip">
                                      <button
                                        onClick={handleGenerateInstruction}
                                        disabled={projectFileGenerating || projectFileSaving || !hasAnthropicKey}
                                        className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                                      >
                                        <Sparkles size={10} className={projectFileGenerating ? "animate-pulse text-brand" : ""} />
                                        {projectFileGenerating ? "Generating…" : "Generate"}
                                      </button>
                                      {!hasAnthropicKey && (
                                        <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                                          Add your Anthropic API key to access
                                        </span>
                                      )}
                                    </span>
                                  <span className="w-px h-3 bg-border-strong/40" />
                                 {!projectFileEditing ? (
                                    <button
                                      onClick={() => setProjectFileEditing(true)}
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

                            {/* Content area */}
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

            {/* ── Context tab (full-bleed, like project_file) ──────────── */}
            {projectTab === "context" && (
              <div className="flex-1 flex flex-col min-h-0">
                {!project?.directory ? (
                  <div className="flex-1 flex items-center justify-center">
                    <p className="text-[13px] text-text-muted italic">
                      Set a project directory to use context.
                    </p>
                  </div>
                ) : loadingContext ? (
                  <div className="flex-1 flex items-center justify-center text-text-muted">
                    <RefreshCw size={14} className="animate-spin mr-2" />
                    <span className="text-[13px]">Loading…</span>
                  </div>
                ) : !contextFileExists && !contextEditing ? (
                  /* ── Create prompt ── */
                  <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
                    <div className="w-12 h-12 mx-auto mb-4 rounded-full border border-dashed border-border-strong flex items-center justify-center text-text-muted">
                      <Brain size={20} strokeWidth={1.5} />
                    </div>
                    <h3 className="text-[14px] font-medium text-text-base mb-1">No context file</h3>
                    <p className="text-[13px] text-text-muted mb-1 max-w-xs">
                      Create <code className="font-mono text-[12px]">.automatic/context.json</code> to give agents structured knowledge about this project.
                    </p>
                    <p className="text-[12px] text-text-muted mb-5 max-w-sm">
                      Define commands, entry points, architecture concepts, conventions, and gotchas.
                    </p>
                    <div className="flex items-center gap-2">
                      <span className="relative group/keytip">
                        <button
                          onClick={handleGenerateContext}
                          disabled={contextGenerating || !hasAnthropicKey}
                          className="px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          <Sparkles size={12} className={contextGenerating ? "animate-pulse" : ""} />
                          {contextGenerating ? "Generating…" : "Generate with AI"}
                        </button>
                        {!hasAnthropicKey && (
                          <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                            Add your Anthropic API key to access
                          </span>
                        )}
                      </span>
                      <button
                        onClick={() => {
                          const template = JSON.stringify({
                            commands: { build: "npm run build", test: "npm test" },
                            entry_points: { app: "src/main.ts" },
                            concepts: { example: { summary: "Describe a key concept here", files: [] } },
                            conventions: { naming: "Describe a naming convention" },
                            gotchas: {},
                          }, null, 2);
                          setContextRaw(template);
                          setContextEditing(true);
                          setContextDirty(true);
                          setContextJsonError(null);
                        }}
                        className="px-3 py-1.5 bg-bg-input hover:bg-surface-hover border border-border-strong/50 text-text-muted hover:text-text-base text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5"
                      >
                        <Plus size={12} /> Create manually
                      </button>
                    </div>
                    {contextJsonError && (
                      <div className="flex items-start gap-2 mt-4 px-4 py-2 bg-error/10 border border-error/30 rounded-lg max-w-sm">
                        <AlertCircle size={12} className="text-error mt-0.5 flex-shrink-0" />
                        <span className="text-[11px] text-error font-mono">{contextJsonError}</span>
                      </div>
                    )}
                  </div>
                ) : (
                  /* ── Editor area ── */
                  <div className="flex-1 flex flex-col min-h-0">
                    {/* Toolbar */}
                    <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
                      <div className="flex items-center gap-2 min-w-0">
                        <span className="text-[11px] text-text-muted font-mono">
                          .automatic/context.json
                          {!contextFileExists ? " (new)" : ""}
                          {contextEditing ? " — Editing" : ""}
                          {contextDirty ? " (unsaved)" : ""}
                        </span>
                        <TokenPill text={contextRaw} />
                      </div>
                      <div className="flex items-center gap-1.5">
                        {/* Generate button — always visible in the toolbar */}
                        <span className="relative group/keytip">
                          <button
                            onClick={handleGenerateContext}
                            disabled={contextGenerating || contextSaving || !hasAnthropicKey}
                            className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            <Sparkles size={10} className={contextGenerating ? "animate-pulse text-brand" : ""} />
                            {contextGenerating ? "Generating…" : "Generate"}
                          </button>
                          {!hasAnthropicKey && (
                            <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                              Add your Anthropic API key to access
                            </span>
                          )}
                        </span>
                        <div className="w-px h-3 bg-border-strong/40" />
                        {!contextEditing ? (
                          <button
                            onClick={() => setContextEditing(true)}
                            className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                          >
                            <Edit2 size={10} /> Edit
                          </button>
                        ) : (
                          <>
                            <button
                              onClick={() => {
                                setContextEditing(false);
                                setContextJsonError(null);
                                if (contextDirty && selectedName) {
                                  if (contextFileExists) {
                                    loadContext(selectedName);
                                  } else {
                                    setContextRaw("");
                                    setContextDirty(false);
                                  }
                                }
                              }}
                              className="px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                            >
                              Cancel
                            </button>
                            <button
                              onClick={handleSaveContext}
                              disabled={!contextDirty || contextSaving}
                              className="flex items-center gap-1 px-2 py-0.5 text-[11px] bg-brand hover:bg-brand-hover text-white rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                              <Check size={10} /> {contextSaving ? "Saving…" : "Save"}
                            </button>
                          </>
                        )}
                      </div>
                    </div>

                    {/* JSON error banner */}
                    {contextJsonError && (
                      <div className="flex items-start gap-2 px-4 py-2 bg-error/10 border-b border-error/30 flex-shrink-0">
                        <AlertCircle size={12} className="text-error mt-0.5 flex-shrink-0" />
                        <span className="text-[11px] text-error font-mono">{contextJsonError}</span>
                      </div>
                    )}

                    {/* Content: raw JSON editor or structured read-only view */}
                    {contextEditing ? (
                      <textarea
                        value={contextRaw}
                        onChange={(e) => {
                          setContextRaw(e.target.value);
                          setContextDirty(true);
                          setContextJsonError(null);
                        }}
                        className="flex-1 w-full p-4 resize-none outline-none font-mono text-[12px] bg-bg-base text-text-base leading-relaxed custom-scrollbar placeholder-text-muted/30 min-h-0"
                        placeholder={`{\n  "commands": {},\n  "concepts": {},\n  "conventions": {},\n  "gotchas": {}\n}`}
                        spellCheck={false}
                      />
                    ) : (
                      <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-5">
                        {(() => {
                          const ctx = projectContext;
                          if (!ctx) return <span className="text-[13px] text-text-muted italic">Empty file.</span>;
                          // eslint-disable-next-line @typescript-eslint/no-explicit-any
                          const sections: any[] = [];

                          if (Object.keys(ctx.commands).length > 0)
                            sections.push(
                              <div key="commands">
                                <div className="flex items-center gap-2 mb-2">
                                  <Code size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Commands</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.commands).map(([name, cmd]) => (
                                    <div key={name} className="flex items-start gap-3 px-4 py-2.5">
                                      <span className="text-[12px] font-medium text-text-base w-32 flex-shrink-0 pt-px">{name}</span>
                                      <code className="text-[11px] font-mono text-text-muted break-all">{cmd}</code>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          if (Object.keys(ctx.entry_points).length > 0)
                            sections.push(
                              <div key="entry_points">
                                <div className="flex items-center gap-2 mb-2">
                                  <ArrowRight size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Entry Points</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.entry_points).map(([name, path]) => (
                                    <div key={name} className="flex items-start gap-3 px-4 py-2.5">
                                      <span className="text-[12px] font-medium text-text-base w-32 flex-shrink-0 pt-px">{name}</span>
                                      <code className="text-[11px] font-mono text-text-muted break-all">{path}</code>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          if (Object.keys(ctx.concepts).length > 0)
                            sections.push(
                              <div key="concepts">
                                <div className="flex items-center gap-2 mb-2">
                                  <Brain size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Architecture Concepts</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.concepts).map(([name, concept]) => (
                                    <div key={name} className="px-4 py-3 space-y-1.5">
                                      <span className="text-[12px] font-semibold text-text-base block">{name}</span>
                                      <p className="text-[12px] text-text-muted leading-relaxed">{concept.summary}</p>
                                      {concept.files.length > 0 && (
                                        <div className="flex flex-wrap gap-1.5">
                                          {concept.files.map((f) => (
                                            <code key={f} className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-bg-sidebar border border-border-strong/30 text-text-muted">{f}</code>
                                          ))}
                                        </div>
                                      )}
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          if (Object.keys(ctx.conventions).length > 0)
                            sections.push(
                              <div key="conventions">
                                <div className="flex items-center gap-2 mb-2">
                                  <ScrollText size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Conventions</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.conventions).map(([name, desc]) => (
                                    <div key={name} className="px-4 py-2.5 space-y-0.5">
                                      <span className="text-[12px] font-medium text-text-base block">{name}</span>
                                      <p className="text-[12px] text-text-muted leading-relaxed">{desc}</p>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          if (Object.keys(ctx.gotchas).length > 0)
                            sections.push(
                              <div key="gotchas">
                                <div className="flex items-center gap-2 mb-2">
                                  <AlertCircle size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Gotchas</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.gotchas).map(([name, desc]) => (
                                    <div key={name} className="px-4 py-2.5 space-y-0.5">
                                      <span className="text-[12px] font-medium text-text-base block">{name}</span>
                                      <p className="text-[12px] text-text-muted leading-relaxed">{desc}</p>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          return sections.length > 0
                            ? <>{sections}</>
                            : <span className="text-[13px] text-text-muted italic">Empty context file. Click Edit to add content.</span>;
                        })()}
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* Features tab — full-height, no padding (handles its own layout) */}
            {projectTab === "features" && projectGroup === "planning" && selectedName && (
              <div className="flex-1 overflow-hidden">
                <Features projectName={selectedName} />
              </div>
            )}

            {/* ── Tools group ──────────────────────────────────────────── */}
            {projectGroup === "tools" && (
              <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
                <div className="space-y-8">
                  {toolTab === null ? (
                    <ProjectToolsTab
                      projectDir={project.directory}
                      projectTools={project.tools ?? []}
                      entries={toolEntries}
                      loading={toolEntriesLoading}
                      onReload={loadToolEntries}
                      onToolsChange={(tools) => {
                        const updated = { ...project, tools, updated_at: new Date().toISOString() };
                        setProject(updated);
                        setDirty(false);
                        saveProjectSnapshot(updated);
                      }}
                    />
                  ) : (
                    (() => {
                      const entry = toolEntries.find((e) => e.name === toolTab);
                      if (!entry) return (
                        <p className="text-[12px] text-text-muted">Tool not found.</p>
                      );
                      return (
                        <ProjectToolDetailPanel
                          entry={entry}
                          projectDir={project.directory}
                          active={(project.tools ?? []).includes(entry.name)}
                          onAdd={() => {
                            const tools = [...new Set([...(project.tools ?? []), entry.name])];
                            const updated = { ...project, tools, updated_at: new Date().toISOString() };
                            setProject(updated);
                            setDirty(false);
                            saveProjectSnapshot(updated);
                          }}
                          onRemove={() => {
                            const tools = (project.tools ?? []).filter((t) => t !== entry.name);
                            const updated = { ...project, tools, updated_at: new Date().toISOString() };
                            setProject(updated);
                            setDirty(false);
                            saveProjectSnapshot(updated);
                          }}
                        />
                      );
                    })()
                  )}
                </div>
              </div>
            )}

            {/* Other tabs (padded container) */}
            {projectGroup !== "tools" && projectTab !== "project_file" && projectTab !== "context" && projectTab !== "features" && (
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="space-y-8">

                {/* ── Rules tab ─────────────────────────────────────────── */}
                {projectTab === "rules" && (() => {
                  const projectRules = (project.file_rules || {})["_project"] || [];
                  const customRules: CustomRule[] = project.custom_rules || [];

                  const handleToggleProjectRule = (ruleId: string) => {
                    const existing = (project.file_rules || {})["_project"] || [];
                    const updated = existing.includes(ruleId)
                      ? existing.filter(r => r !== ruleId)
                      : [...existing, ruleId];
                    const newFileRules: Record<string, string[]> = { ...(project.file_rules || {}), _project: updated };
                    if (updated.length === 0) delete newFileRules["_project"];
                    setProject({ ...project, file_rules: newFileRules });
                    setDirty(true);
                  };

                  const handleAddCustomRule = () => {
                    const newRule: CustomRule = { name: "New Rule", content: "" };
                    setProject({ ...project, custom_rules: [...customRules, newRule] });
                    setCustomRuleEditingIdx(customRules.length);
                    setCustomRuleEditName("New Rule");
                    setCustomRuleEditContent("");
                    setDirty(true);
                  };

                  const handleDeleteCustomRule = (idx: number) => {
                    const updated = customRules.filter((_, i) => i !== idx);
                    setProject({ ...project, custom_rules: updated });
                    if (customRuleEditingIdx === idx) {
                      setCustomRuleEditingIdx(null);
                    } else if (customRuleEditingIdx !== null && customRuleEditingIdx > idx) {
                      setCustomRuleEditingIdx(customRuleEditingIdx - 1);
                    }
                    setDirty(true);
                  };

                  const handleStartEditCustomRule = (idx: number) => {
                    setCustomRuleEditingIdx(idx);
                    setCustomRuleEditName(customRules[idx]?.name ?? "");
                    setCustomRuleEditContent(customRules[idx]?.content ?? "");
                  };

                  const handleCommitCustomRule = () => {
                    if (customRuleEditingIdx === null) return;
                    const updated = customRules.map((r, i) =>
                      i === customRuleEditingIdx
                        ? { name: customRuleEditName.trim() || "Untitled Rule", content: customRuleEditContent }
                        : r
                    );
                    setProject({ ...project, custom_rules: updated });
                    setCustomRuleEditingIdx(null);
                    setDirty(true);
                  };

                  const totalActive = projectRules.length + customRules.filter(r => r.content.trim()).length;

                  return (
                    <div className="space-y-8">

                      {/* ── Section header ── */}
                      <div className="flex items-center justify-between">
                        <div>
                          <h2 className="text-[15px] font-semibold text-text-base">Rules</h2>
                          <p className="text-[12px] text-text-muted mt-0.5">
                            Rules are injected into all agent instruction files when the project is synced.
                          </p>
                        </div>
                        {totalActive > 0 && (
                          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
                            {totalActive} active
                          </span>
                        )}
                      </div>

                      {/* ── Custom Rules ── */}
                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <Edit2 size={13} className="text-text-muted" />
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Rules</span>
                            {customRules.length > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {customRules.length}
                              </span>
                            )}
                          </div>
                          <button
                            onClick={handleAddCustomRule}
                            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                          >
                            <Plus size={12} /> Add Rule
                          </button>
                        </div>
                        <p className="text-[12px] text-text-muted mb-3">
                          Write rules directly in this project. They are injected alongside any global rules selected below.
                        </p>

                        {customRules.length === 0 ? (
                          <button
                            onClick={handleAddCustomRule}
                            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
                          >
                            <Plus size={14} /> Write your first project rule
                          </button>
                        ) : (
                          <div className="space-y-2">
                            {customRules.map((rule, idx) => {
                              const isEditing = customRuleEditingIdx === idx;
                              return (
                                <div
                                  key={idx}
                                  className={`rounded-lg border transition-colors ${
                                    isEditing
                                      ? "border-brand/40 bg-bg-input"
                                      : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                                  }`}
                                >
                                  {isEditing ? (
                                    /* ── Edit mode ── */
                                    <div className="p-3 space-y-2">
                                      <input
                                        type="text"
                                        value={customRuleEditName}
                                        onChange={(e) => setCustomRuleEditName(e.target.value)}
                                        placeholder="Rule name"
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                                      />
                                      <textarea
                                        value={customRuleEditContent}
                                        onChange={(e) => setCustomRuleEditContent(e.target.value)}
                                        placeholder="Write the rule content in Markdown…"
                                        rows={8}
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-2 text-[12px] font-mono text-text-base placeholder-text-muted/50 outline-none resize-y transition-colors leading-relaxed"
                                      />
                                      <div className="flex items-center justify-end gap-2 pt-1">
                                        <button
                                          onClick={() => setCustomRuleEditingIdx(null)}
                                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                                        >
                                          Cancel
                                        </button>
                                        <button
                                          onClick={handleCommitCustomRule}
                                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                                        >
                                          <Check size={11} /> Save
                                        </button>
                                      </div>
                                    </div>
                                  ) : (
                                    /* ── View mode ── */
                                    <div className="flex items-center gap-3 px-3 py-2.5">
                                      <ScrollText size={14} className="flex-shrink-0 text-text-muted" />
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-text-base truncate">{rule.name || "Untitled Rule"}</div>
                                        {rule.content.trim() ? (
                                          <div className="text-[11px] text-text-muted truncate mt-0.5">
                                            {rule.content.trim().split("\n")[0]}
                                          </div>
                                        ) : (
                                          <div className="text-[11px] text-text-muted/60 italic mt-0.5">Empty — add content to activate</div>
                                        )}
                                      </div>
                                      <TokenPill text={rule.content} />
                                      <div className="flex items-center gap-1 flex-shrink-0">
                                        <button
                                          onClick={() => handleStartEditCustomRule(idx)}
                                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                          title="Edit"
                                        >
                                          <Edit2 size={12} />
                                        </button>
                                        <button
                                          onClick={() => handleDeleteCustomRule(idx)}
                                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                                          title="Delete"
                                        >
                                          <Trash2 size={12} />
                                        </button>
                                      </div>
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {/* ── Divider ── */}
                      <div className="border-t border-border-strong/30" />

                      {/* ── Global Rules ── */}
                      {(() => {
                        const unaddedRules = availableRules.filter(r => !projectRules.includes(r.id));
                        const filteredRules = globalRuleSearch.trim()
                          ? unaddedRules.filter(r =>
                              r.name.toLowerCase().includes(globalRuleSearch.toLowerCase()) ||
                              r.id.toLowerCase().includes(globalRuleSearch.toLowerCase())
                            )
                          : unaddedRules;

                        return (
                          <section>
                            {/* Header */}
                            <div className="flex items-center justify-between mb-3">
                              <div className="flex items-center gap-2">
                                <ScrollText size={13} className="text-text-muted" />
                                <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Global Rules</span>
                              </div>
                              {availableRules.length > 0 && (
                                <button
                                  onClick={() => setGlobalRuleAdding(true)}
                                  className="text-[11px] text-brand hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
                                >
                                  <Plus size={11} /> Add
                                </button>
                              )}
                            </div>

                            {/* Selected rules list */}
                            {projectRules.length === 0 && !globalRuleAdding && (
                              <p className="text-[12px] text-text-muted italic pl-1">No global rules selected.</p>
                            )}
                            <div className="space-y-2">
                              {projectRules.map((ruleId) => {
                                const meta = availableRules.find(r => r.id === ruleId);
                                return (
                                  <div
                                    key={ruleId}
                                    className="bg-bg-input border border-border-strong/40 rounded-lg group flex items-center gap-3 px-3 py-3"
                                  >
                                    <div className="w-8 h-8 rounded-md bg-brand/10 flex items-center justify-center flex-shrink-0">
                                      <ScrollText size={15} className="text-brand" />
                                    </div>
                                    <div className="flex-1 min-w-0">
                                      <div className="text-[13px] font-medium text-text-base truncate">
                                        {meta?.name ?? ruleId}
                                      </div>
                                      <div className="text-[11px] text-text-muted truncate">{ruleId}</div>
                                    </div>
                                    <TokenPill text={globalRuleContentCache[ruleId] ?? ""} />
                                    <button
                                      onClick={() => handleToggleProjectRule(ruleId)}
                                      className="text-text-muted hover:text-danger opacity-100 transition-all p-1 hover:bg-surface rounded"
                                      title="Remove"
                                    >
                                      <Trash2 size={12} />
                                    </button>
                                  </div>
                                );
                              })}
                            </div>

                            {/* Searchable add dropdown */}
                            {globalRuleAdding && (
                              <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
                                <div className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40">
                                  <Search size={12} className="text-text-muted shrink-0" />
                                  <input
                                    type="text"
                                    value={globalRuleSearch}
                                    onChange={(e) => setGlobalRuleSearch(e.target.value)}
                                    onKeyDown={(e) => {
                                      if (e.key === "Escape") { setGlobalRuleAdding(false); setGlobalRuleSearch(""); }
                                      if (e.key === "Enter" && filteredRules.length === 1) {
                                        handleToggleProjectRule(filteredRules[0]!.id);
                                        setGlobalRuleAdding(false);
                                        setGlobalRuleSearch("");
                                      }
                                    }}
                                    placeholder="Search rules…"
                                    autoFocus
                                    className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
                                  />
                                  {globalRuleSearch && (
                                    <button
                                      onClick={() => setGlobalRuleSearch("")}
                                      className="text-text-muted hover:text-text-base transition-colors"
                                    >
                                      <X size={11} />
                                    </button>
                                  )}
                                </div>
                                <div className="max-h-48 overflow-y-auto custom-scrollbar py-1">
                                  {filteredRules.length > 0 ? (
                                    filteredRules.map((r) => (
                                      <button
                                        key={r.id}
                                        onClick={() => {
                                          handleToggleProjectRule(r.id);
                                          setGlobalRuleAdding(false);
                                          setGlobalRuleSearch("");
                                        }}
                                        className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-bg-sidebar text-left transition-colors"
                                      >
                                        <div className="w-5 h-5 rounded bg-brand/10 flex items-center justify-center flex-shrink-0">
                                          <ScrollText size={11} className="text-brand" />
                                        </div>
                                        <div className="min-w-0">
                                          <div className="text-[13px] text-text-base truncate">{r.name}</div>
                                          <div className="text-[11px] text-text-muted truncate">{r.id}</div>
                                        </div>
                                      </button>
                                    ))
                                  ) : (
                                    <p className="text-[12px] text-text-muted italic px-3 py-3">
                                      {unaddedRules.length === 0 ? "All rules already added." : "No rules match."}
                                    </p>
                                  )}
                                </div>
                                <div className="border-t border-border-strong/40 px-3 py-2 flex items-center justify-between">
                                  <span className="text-[11px] text-text-muted">
                                    {filteredRules.length} of {unaddedRules.length} rule{unaddedRules.length !== 1 ? "s" : ""}
                                  </span>
                                  <button
                                    onClick={() => { setGlobalRuleAdding(false); setGlobalRuleSearch(""); }}
                                    className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                                  >
                                    Cancel
                                  </button>
                                </div>
                              </div>
                            )}

                            {availableRules.length === 0 && (
                              <div className="px-4 py-6 bg-bg-input border border-border-strong/40 rounded-lg text-center">
                                <ScrollText size={18} className="mx-auto mb-2 text-text-muted" strokeWidth={1.5} />
                                <p className="text-[13px] text-text-muted mb-1">No global rules yet.</p>
                                <p className="text-[12px] text-text-muted/70">Create reusable rules in the Rules section of the sidebar.</p>
                              </div>
                            )}
                          </section>
                        );
                      })()}

                      {dirty && (
                        <div className="flex justify-end">
                          <button
                            onClick={handleSave}
                            disabled={syncStatus === "syncing"}
                            className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
                          >
                            <Check size={13} /> {syncStatus === "syncing" ? "Saving…" : "Save Changes"}
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })()}

                {/* ── Commands tab ─────────────────────────────────────────── */}
                {projectTab === "commands" && (() => {
                  const customCommands: CustomCommand[] = project.custom_commands || [];

                  const handleAddCustomCommand = () => {
                    const newCommand: CustomCommand = {
                      name: "new-command",
                      content: "---\ndescription: Describe what this command does.\n---\n\nWrite the reusable prompt here.\n",
                    };
                    setProject({ ...project, custom_commands: [...customCommands, newCommand] });
                    setCustomCommandEditingIdx(customCommands.length);
                    setCustomCommandEditName(newCommand.name);
                    setCustomCommandEditContent(newCommand.content);
                    setDirty(true);
                  };

                  const handleDeleteCustomCommand = (idx: number) => {
                    const updated = customCommands.filter((_, i) => i !== idx);
                    setProject({ ...project, custom_commands: updated.length > 0 ? updated : undefined });
                    if (customCommandEditingIdx === idx) {
                      setCustomCommandEditingIdx(null);
                    } else if (customCommandEditingIdx !== null && customCommandEditingIdx > idx) {
                      setCustomCommandEditingIdx(customCommandEditingIdx - 1);
                    }
                    setDirty(true);
                  };

                  const handleStartEditCustomCommand = (idx: number) => {
                    setCustomCommandEditingIdx(idx);
                    setCustomCommandEditName(customCommands[idx]?.name ?? "");
                    setCustomCommandEditContent(customCommands[idx]?.content ?? "");
                  };

                  const handleCommitCustomCommand = () => {
                    if (customCommandEditingIdx === null) return;
                    const updated = customCommands.map((command, i) =>
                      i === customCommandEditingIdx
                        ? {
                            name: customCommandEditName.trim() || "untitled-command",
                            content: customCommandEditContent,
                          }
                        : command
                    );
                    setProject({ ...project, custom_commands: updated });
                    setCustomCommandEditingIdx(null);
                    setDirty(true);
                  };

                  return (
                    <div className="space-y-8">
                      <div className="flex items-center justify-between">
                        <div>
                          <h2 className="text-[15px] font-semibold text-text-base">Commands</h2>
                        </div>
                        {((project.user_commands?.length ?? 0) + customCommands.length) > 0 && (
                          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
                            {(project.user_commands?.length ?? 0) + customCommands.length} commands
                          </span>
                        )}
                      </div>

                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <Terminal size={13} className="text-text-muted" />
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Commands</span>
                            {customCommands.length > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {customCommands.length}
                              </span>
                            )}
                          </div>
                          <button
                            onClick={handleAddCustomCommand}
                            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                          >
                            <Plus size={12} /> Add Command
                          </button>
                        </div>

                        {customCommands.length === 0 ? (
                          <button
                            onClick={handleAddCustomCommand}
                            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
                          >
                            <Plus size={14} /> Create your first project command
                          </button>
                        ) : (
                          <div className="space-y-2">
                            {customCommands.map((command, idx) => {
                              const isEditing = customCommandEditingIdx === idx;
                              return (
                                <div
                                  key={`${command.name}-${idx}`}
                                  className={`rounded-lg border transition-colors ${
                                    isEditing
                                      ? "border-brand/40 bg-bg-input"
                                      : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                                  }`}
                                >
                                  {isEditing ? (
                                    <div className="p-3 space-y-2">
                                      <input
                                        type="text"
                                        value={customCommandEditName}
                                        onChange={(e) => setCustomCommandEditName(e.target.value)}
                                        placeholder="command-name"
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                                      />
                                      <textarea
                                        value={customCommandEditContent}
                                        onChange={(e) => setCustomCommandEditContent(e.target.value)}
                                        placeholder="Write the command as Markdown with optional YAML frontmatter..."
                                        rows={12}
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-2 text-[12px] font-mono text-text-base placeholder-text-muted/50 outline-none resize-y transition-colors leading-relaxed"
                                      />
                                      <div className="flex items-center justify-end gap-2 pt-1">
                                        <button
                                          onClick={() => setCustomCommandEditingIdx(null)}
                                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                                        >
                                          Cancel
                                        </button>
                                        <button
                                          onClick={handleCommitCustomCommand}
                                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                                        >
                                          <Check size={11} /> Save
                                        </button>
                                      </div>
                                    </div>
                                  ) : (
                                    <div className="flex items-center gap-3 px-3 py-2.5">
                                      <Terminal size={14} className="flex-shrink-0 text-text-muted" />
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-text-base truncate">/{command.name || "untitled-command"}</div>
                                        <div className="text-[11px] text-text-muted truncate mt-0.5">
                                          {command.content.trim().split("\n").find((line) => line.trim() && !line.startsWith("---"))?.slice(0, 80) || "Custom command"}
                                        </div>
                                      </div>
                                      <TokenPill text={command.content} />
                                      <div className="flex items-center gap-1 flex-shrink-0">
                                        <button
                                          onClick={() => handleStartEditCustomCommand(idx)}
                                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                          title="Edit"
                                        >
                                          <Edit2 size={12} />
                                        </button>
                                        <button
                                          onClick={() => handleDeleteCustomCommand(idx)}
                                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                                          title="Delete"
                                        >
                                          <Trash2 size={12} />
                                        </button>
                                      </div>
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <div className="p-1 bg-icon-agent/10 rounded"><Globe size={12} className="text-icon-agent" /></div>
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Workspace Commands</span>
                            {(project.user_commands?.length ?? 0) > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {project.user_commands?.length ?? 0}
                              </span>
                            )}
                          </div>
                          <div className="relative">
                            <button
                              onClick={() => setUserCommandAdding(!userCommandAdding)}
                              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                            >
                              <Plus size={12} /> Add from Library
                            </button>
                            {userCommandAdding && (
                              <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
                                <div className="p-2 border-b border-border-strong/40">
                                  <input
                                    type="text"
                                    value={userCommandSearch}
                                    onChange={(e) => setUserCommandSearch(e.target.value)}
                                    placeholder="Search commands..."
                                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                                    autoFocus
                                  />
                                </div>
                                <div className="py-1">
                                  {availableUserCommands
                                    .filter((command) => {
                                      const search = userCommandSearch.toLowerCase();
                                      return (
                                        command.id.toLowerCase().includes(search) ||
                                        command.description.toLowerCase().includes(search)
                                      );
                                    })
                                    .filter((command) => !(project.user_commands ?? []).includes(command.id))
                                    .length === 0 ? (
                                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                                      {availableUserCommands.length === 0
                                        ? "No workspace commands available"
                                        : "All commands already added"}
                                    </div>
                                  ) : (
                                    availableUserCommands
                                      .filter((command) => {
                                        const search = userCommandSearch.toLowerCase();
                                        return (
                                          command.id.toLowerCase().includes(search) ||
                                          command.description.toLowerCase().includes(search)
                                        );
                                      })
                                      .filter((command) => !(project.user_commands ?? []).includes(command.id))
                                      .map((command) => (
                                        <button
                                          key={command.id}
                                          onClick={() => {
                                            const currentUserCommands = project.user_commands ?? [];
                                            setProject({
                                              ...project,
                                              user_commands: [...currentUserCommands, command.id],
                                            });
                                            setDirty(true);
                                            setUserCommandAdding(false);
                                            setUserCommandSearch("");
                                          }}
                                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                                        >
                                          <Terminal size={14} className="text-text-muted flex-shrink-0" />
                                          <div className="min-w-0">
                                            <div className="text-[12px] font-medium text-text-base truncate">
                                              /{command.id}
                                            </div>
                                            <div className="text-[11px] text-text-muted truncate">
                                              {command.description || "No description"}
                                            </div>
                                          </div>
                                        </button>
                                      ))
                                  )}
                                </div>
                              </div>
                            )}
                          </div>
                        </div>

                        {(project.user_commands?.length ?? 0) === 0 ? (
                          <div className="text-[12px] text-text-muted/60 italic py-4 text-center">
                            No workspace commands selected. Add commands from your library to include them in this project.
                          </div>
                        ) : (
                          <div className="space-y-1">
                            {project.user_commands?.map((commandId) => {
                              const command = availableUserCommands.find((entry) => entry.id === commandId);
                              return (
                                <div
                                  key={commandId}
                                  className="flex items-center gap-3 px-3 py-2 bg-bg-input border border-border-strong/40 hover:border-border-strong rounded-lg transition-colors"
                                >
                                  <Terminal size={14} className="flex-shrink-0 text-text-muted" />
                                  <div className="flex-1 min-w-0">
                                    <div className="text-[13px] font-medium text-text-base truncate">
                                      /{command?.id ?? commandId}
                                    </div>
                                    <div className="text-[11px] text-text-muted truncate">
                                      {command?.description || commandId}
                                    </div>
                                  </div>
                                  <button
                                    onClick={() => {
                                      const updated = (project.user_commands ?? []).filter((id) => id !== commandId);
                                      setProject({ ...project, user_commands: updated.length > 0 ? updated : undefined });
                                      setDirty(true);
                                    }}
                                    className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0"
                                    title="Remove"
                                  >
                                    <X size={12} />
                                  </button>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {dirty && (
                        <div className="flex justify-end">
                          <button
                            onClick={handleSave}
                            disabled={syncStatus === "syncing"}
                            className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
                          >
                            <Check size={13} /> {syncStatus === "syncing" ? "Saving..." : "Save Changes"}
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })()}

                {/* ── Agents tab (custom_agents) ───────────────────────────── */}
                {projectTab === "custom_agents" && (() => {
                  const customAgents: CustomAgent[] = project.custom_agents || [];

                  const handleAddCustomAgent = () => {
                    const newAgent: CustomAgent = {
                      name: "New Agent",
                      content: "---\nname: new-agent\ndescription: A specialized AI assistant.\ntools: Read, Grep, Glob, Bash\nmodel: inherit\n---\n\nYou are a specialized AI assistant.\n"
                    };
                    setProject({ ...project, custom_agents: [...customAgents, newAgent] });
                    setCustomAgentEditingIdx(customAgents.length);
                    setCustomAgentEditName("New Agent");
                    setCustomAgentEditContent(newAgent.content);
                    setDirty(true);
                  };

                  const handleDeleteCustomAgent = (idx: number) => {
                    const updated = customAgents.filter((_, i) => i !== idx);
                    setProject({ ...project, custom_agents: updated.length > 0 ? updated : undefined });
                    if (customAgentEditingIdx === idx) {
                      setCustomAgentEditingIdx(null);
                    } else if (customAgentEditingIdx !== null && customAgentEditingIdx > idx) {
                      setCustomAgentEditingIdx(customAgentEditingIdx - 1);
                    }
                    setDirty(true);
                  };

                  const handleStartEditCustomAgent = (idx: number) => {
                    setCustomAgentEditingIdx(idx);
                    setCustomAgentEditName(customAgents[idx]?.name ?? "");
                    setCustomAgentEditContent(customAgents[idx]?.content ?? "");
                  };

                  const handleCommitCustomAgent = () => {
                    if (customAgentEditingIdx === null) return;
                    const updated = customAgents.map((a, i) =>
                      i === customAgentEditingIdx
                        ? { name: customAgentEditName.trim() || "Untitled Agent", content: customAgentEditContent }
                        : a
                    );
                    setProject({ ...project, custom_agents: updated });
                    setCustomAgentEditingIdx(null);
                    setDirty(true);
                  };

                  return (
                    <div className="space-y-8">
                      {/* ── Section header ── */}
                      <div className="flex items-center justify-between">
                        <div>
                          <h2 className="text-[15px] font-semibold text-text-base">Agents</h2>
                        </div>
                        {customAgents.length > 0 && (
                          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
                            {customAgents.length} {customAgents.length === 1 ? "agent" : "agents"}
                          </span>
                        )}
                      </div>

                      {/* ── Custom Agents ── */}
                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <MessagesSquare size={13} className="text-text-muted" />
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Agents</span>
                            {customAgents.length > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {customAgents.length}
                              </span>
                            )}
                          </div>
                          <button
                            onClick={handleAddCustomAgent}
                            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                          >
                            <Plus size={12} /> Add Agent
                          </button>
                        </div>

                        {customAgents.length === 0 ? (
                          <button
                            onClick={handleAddCustomAgent}
                            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
                          >
                            <Plus size={14} /> Create your first project agent
                          </button>
                        ) : (
                          <div className="space-y-2">
                            {customAgents.map((agent, idx) => {
                              const isEditing = customAgentEditingIdx === idx;
                              return (
                                <div
                                  key={idx}
                                  className={`rounded-lg border transition-colors ${
                                    isEditing
                                      ? "border-brand/40 bg-bg-input"
                                      : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                                  }`}
                                >
                                  {isEditing ? (
                                    <div className="p-3 space-y-2">
                                      <input
                                        type="text"
                                        value={customAgentEditName}
                                        onChange={(e) => setCustomAgentEditName(e.target.value)}
                                        placeholder="Agent display name"
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                                      />
                                      <textarea
                                        value={customAgentEditContent}
                                        onChange={(e) => setCustomAgentEditContent(e.target.value)}
                                        placeholder="Write the agent content as Markdown with YAML frontmatter..."
                                        rows={12}
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-2 text-[12px] font-mono text-text-base placeholder-text-muted/50 outline-none resize-y transition-colors leading-relaxed"
                                      />
                                      <div className="flex items-center justify-end gap-2 pt-1">
                                        <button
                                          onClick={() => setCustomAgentEditingIdx(null)}
                                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                                        >
                                          Cancel
                                        </button>
                                        <button
                                          onClick={handleCommitCustomAgent}
                                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                                        >
                                          <Check size={11} /> Save
                                        </button>
                                      </div>
                                    </div>
                                  ) : (
                                    <div className="flex items-center gap-3 px-3 py-2.5">
                                      <MessagesSquare size={14} className="flex-shrink-0 text-text-muted" />
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-text-base truncate">{agent.name || "Untitled Agent"}</div>
                                        {agent.content.trim() ? (
                                          <div className="text-[11px] text-text-muted truncate mt-0.5">
                                            {agent.content.trim().split("\n").find(l => l.trim() && !l.startsWith("---"))?.slice(0, 60) || "Custom agent"}
                                          </div>
                                        ) : (
                                          <div className="text-[11px] text-text-muted/60 italic mt-0.5">Empty</div>
                                        )}
                                      </div>
                                      <TokenPill text={agent.content} />
                                      <div className="flex items-center gap-1 flex-shrink-0">
                                        <button
                                          onClick={() => handleStartEditCustomAgent(idx)}
                                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                          title="Edit"
                                        >
                                          <Edit2 size={12} />
                                        </button>
                                        <button
                                          onClick={() => handleDeleteCustomAgent(idx)}
                                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                                          title="Delete"
                                        >
                                          <Trash2 size={12} />
                                        </button>
                                      </div>
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {/* ── Workspace Agents (from ~/.automatic/agents/) ── */}
                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <div className="p-1 bg-icon-agent/10 rounded"><Globe size={12} className="text-icon-agent" /></div>
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Workspace Agents</span>
                            {(project.user_agents?.length ?? 0) > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {project.user_agents?.length ?? 0}
                              </span>
                            )}
                          </div>
                          <div className="relative" ref={userAgentDropdownRef}>
                            <button
                              onClick={() => setUserAgentAdding(!userAgentAdding)}
                              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                            >
                              <Plus size={12} /> Add from Library
                            </button>
                            {userAgentAdding && (
                              <div className="absolute right-0 top-full mt-1 w-64 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-64 overflow-y-auto">
                                <div className="p-2 border-b border-border-strong/40">
                                  <input
                                    type="text"
                                    value={userAgentSearch}
                                    onChange={(e) => setUserAgentSearch(e.target.value)}
                                    placeholder="Search agents..."
                                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                                    autoFocus
                                  />
                                </div>
                                <div className="py-1">
                                  {availableUserAgents
                                    .filter((a) => {
                                      const search = userAgentSearch.toLowerCase();
                                      return (
                                        a.name.toLowerCase().includes(search) ||
                                        a.id.toLowerCase().includes(search)
                                      );
                                    })
                                    .filter((a) => !(project.user_agents ?? []).includes(a.id))
                                    .length === 0 ? (
                                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                                      {availableUserAgents.length === 0
                                        ? "No workspace agents available"
                                        : "All agents already added"}
                                    </div>
                                  ) : (
                                    availableUserAgents
                                      .filter((a) => {
                                        const search = userAgentSearch.toLowerCase();
                                        return (
                                          a.name.toLowerCase().includes(search) ||
                                          a.id.toLowerCase().includes(search)
                                        );
                                      })
                                      .filter((a) => !(project.user_agents ?? []).includes(a.id))
                                      .map((agent) => (
                                        <button
                                          key={agent.id}
                                          onClick={() => {
                                            const currentUserAgents = project.user_agents ?? [];
                                            setProject({
                                              ...project,
                                              user_agents: [...currentUserAgents, agent.id],
                                            });
                                            setDirty(true);
                                            setUserAgentAdding(false);
                                            setUserAgentSearch("");
                                          }}
                                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                                        >
                                          <MessagesSquare size={14} className="text-text-muted flex-shrink-0" />
                                          <div className="min-w-0">
                                            <div className="text-[12px] font-medium text-text-base truncate">
                                              {agent.name}
                                            </div>
                                            <div className="text-[11px] text-text-muted truncate">
                                              {agent.id}
                                            </div>
                                          </div>
                                        </button>
                                      ))
                                  )}
                                </div>
                              </div>
                            )}
                          </div>
                        </div>

                        {(project.user_agents?.length ?? 0) === 0 ? (
                          <div className="text-[12px] text-text-muted/60 italic py-4 text-center">
                            No workspace agents selected. Add agents from your library to include them in this project.
                          </div>
                        ) : (
                          <div className="space-y-1">
                            {project.user_agents?.map((agentId) => {
                              const agent = availableUserAgents.find((a) => a.id === agentId);
                              return (
                                <div
                                  key={agentId}
                                  className="flex items-center gap-3 px-3 py-2 bg-bg-input border border-border-strong/40 hover:border-border-strong rounded-lg transition-colors"
                                >
                                  <MessagesSquare size={14} className="flex-shrink-0 text-text-muted" />
                                  <div className="flex-1 min-w-0">
                                    <div className="text-[13px] font-medium text-text-base truncate">
                                      {agent?.name ?? agentId}
                                    </div>
                                    <div className="text-[11px] text-text-muted truncate">
                                      {agentId}
                                    </div>
                                  </div>
                                  <button
                                    onClick={() => {
                                      const updated = (project.user_agents ?? []).filter((id) => id !== agentId);
                                      setProject({ ...project, user_agents: updated.length > 0 ? updated : undefined });
                                      setDirty(true);
                                    }}
                                    className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0"
                                    title="Remove"
                                  >
                                    <X size={12} />
                                  </button>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {dirty && (
                        <div className="flex justify-end">
                          <button
                            onClick={handleSave}
                            disabled={syncStatus === "syncing"}
                            className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
                          >
                            <Check size={13} /> {syncStatus === "syncing" ? "Saving..." : "Save Changes"}
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })()}

                {/* ── Summary tab ──────────────────────────────────────── */}
                {projectTab === "summary" && (() => {
                  const totalSkills = project.skills.length + project.local_skills.length;
                  const totalRules = ((project.file_rules || {})["_project"] || []).length + (project.custom_rules?.length ?? 0);
                  const totalSubAgents = (project.user_agents?.length ?? 0) + (project.custom_agents?.length ?? 0);
                  const totalCommands = (project.user_commands?.length ?? 0) + (project.custom_commands?.length ?? 0);
                  const memoryCount = Object.keys(memories).length;
                  const hasInstructionFiles = projectFiles.some((file) => file.exists);
                  const instructionStatus = !project.directory || project.agents.length === 0
                    ? "Add a directory and at least one agent to generate instruction files."
                    : hasInstructionFiles
                      ? `${projectFiles.filter((file) => file.exists).length} instruction file${projectFiles.filter((file) => file.exists).length === 1 ? "" : "s"} available.`
                      : "No instruction files found for this project yet.";
                  const recentDocsLinks = linkDocEntries.slice(0, 5);

                  return (
                    <div className="space-y-6">
                      <div className="grid gap-4 xl:grid-cols-5 md:grid-cols-2">
                        <SummaryMetricCard
                          icon={<Code size={13} className="text-icon-skill" />}
                          label="Skills"
                          count={totalSkills}
                          accentClass="bg-icon-skill/10"
                          onView={() => selectTab("skills")}
                        />
                        <SummaryMetricCard
                          icon={<Server size={13} className="text-icon-mcp" />}
                          label="MCP Servers"
                          count={project.mcp_servers.length}
                          accentClass="bg-icon-mcp/10"
                          onView={() => selectTab("mcp_servers")}
                        />
                        <SummaryMetricCard
                          icon={<ScrollText size={13} className="text-icon-rule" />}
                          label="Rules"
                          count={totalRules}
                          accentClass="bg-icon-rule/10"
                          onView={() => selectTab("rules")}
                        />
                        <SummaryMetricCard
                          icon={<Bot size={13} className="text-brand" />}
                          label="Sub-agents"
                          count={totalSubAgents}
                          accentClass="bg-brand/10"
                          onView={() => selectTab("custom_agents")}
                        />
                        <SummaryMetricCard
                          icon={<Terminal size={13} className="text-text-base" />}
                          label="Commands"
                          count={totalCommands}
                          accentClass="bg-text-muted/10"
                          onView={() => selectTab("commands")}
                        />
                      </div>

                      <div className="grid grid-cols-[minmax(0,1fr)_280px] gap-6 max-xl:grid-cols-1">

                        {/* ── Column 1: Activity, recommendations, setup ──── */}
                        <div className="space-y-6 min-w-0">

                          {/* Recommendations banner */}
                          {recsDisplayCount > 0 && (
                            <div className="flex items-center gap-3 px-4 py-3 rounded-lg bg-warning/5 border border-warning/25">
                              <Lightbulb size={14} className="text-warning shrink-0" />
                              <p className="flex-1 text-[12px] text-text-muted leading-snug">
                                <span className="font-semibold text-text-base">
                                  {recsDisplayCount === 1 ? "1 recommendation" : `${recsDisplayCount} recommendations`}
                                </span>
                                {" "}available for this project.
                              </p>
                              <button
                                onClick={() => selectTab("recommendations")}
                                className="shrink-0 flex items-center gap-1 text-[12px] font-medium text-warning hover:text-warning-hover transition-colors"
                              >
                                Review <ArrowRight size={11} />
                              </button>
                            </div>
                          )}

                          {/* Getting Started callout (incomplete setup) */}
                          {!isCreating && (!project.directory || project.agents.length === 0) && (
                            <section className="bg-gradient-to-br from-brand/10 to-brand/5 border border-brand/20 rounded-lg p-5">
                              <div className="flex items-start gap-3">
                                <div className="p-2 bg-brand/20 rounded-lg flex-shrink-0">
                                  <Package size={18} className="text-brand" />
                                </div>
                                <div>
                                  <h3 className="text-[13px] font-semibold text-text-base mb-2">Complete Setup</h3>
                                  <p className="text-[12px] text-text-muted mb-3 leading-relaxed">To start using this project, complete these steps:</p>
                                  <ol className="space-y-2 text-[12px] text-text-base">
                                    {!project.directory && (
                                      <li className="flex items-start gap-2">
                                        <div className="w-5 h-5 rounded-full border border-brand flex items-center justify-center flex-shrink-0 mt-0.5">
                                          <span className="text-[10px] text-brand">1</span>
                                        </div>
                                        <div>
                                          <button
                                            onClick={async () => {
                                              const selected: string | null = await invoke("open_directory_dialog");
                                              if (selected) updateField("directory", selected);
                                            }}
                                            className="text-brand hover:text-brand-hover transition-colors font-medium"
                                          >
                                            Set project directory
                                          </button>
                                          <div className="text-[11px] text-text-muted mt-0.5">Click the path below the project name, or click here</div>
                                        </div>
                                      </li>
                                    )}
                                    {project.agents.length === 0 && (
                                      <li className="flex items-start gap-2">
                                        <div className="w-5 h-5 rounded-full border border-brand flex items-center justify-center flex-shrink-0 mt-0.5">
                                          <span className="text-[10px] text-brand">{!project.directory ? "2" : "1"}</span>
                                        </div>
                                        <div>
                                          <button onClick={() => selectTab("agents")} className="text-brand hover:text-brand-hover transition-colors font-medium">Add agent tools</button>
                                          <div className="text-[11px] text-text-muted mt-0.5">Select which agents will use this project</div>
                                        </div>
                                      </li>
                                    )}
                                    <li className="flex items-start gap-2">
                                      <div className="w-5 h-5 rounded-full border border-text-muted/50 flex items-center justify-center flex-shrink-0 mt-0.5">
                                        <span className="text-[10px] text-text-muted">•</span>
                                      </div>
                                      <div>
                                         <button onClick={() => selectTab("skills")} className="text-text-base hover:text-brand transition-colors">Add skills (optional)</button>
                                        <div className="text-[11px] text-text-muted mt-0.5">Give agents specialized capabilities</div>
                                      </div>
                                    </li>
                                  </ol>
                                </div>
                              </div>
                            </section>
                          )}

                          {/* Activity */}
                          <ActivityFeed entries={activityEntries} loading={loadingActivity} />
                        </div>

                        {/* ── Column 2: project sidebar ───────────────────── */}
                        <div className="space-y-4">
                          <SummarySidebarSection title="Instructions">
                            <div className="flex items-start justify-between gap-3">
                              <p className="text-[12px] leading-relaxed text-text-muted">{instructionStatus}</p>
                              <span className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold ${hasInstructionFiles ? "bg-success/10 text-success border border-success/20" : "bg-warning/10 text-warning border border-warning/20"}`}>
                                {hasInstructionFiles ? "Set" : "Missing"}
                              </span>
                            </div>
                            <button
                              onClick={() => selectTab("project_file")}
                              className="text-[11px] font-medium text-text-muted transition-colors hover:text-text-base"
                            >
                              View instructions
                            </button>
                          </SummarySidebarSection>

                          <SummarySidebarSection title="Groups">
                            {loadingGroups ? (
                              <p className="text-[12px] text-text-muted">Loading groups…</p>
                            ) : projectGroupMemberships.length === 0 ? (
                              <p className="text-[12px] text-text-muted">This project is not in any groups.</p>
                            ) : (
                              <div className="flex flex-wrap gap-2">
                                {projectGroupMemberships.map((groupName) => (
                                  <button
                                    key={groupName}
                                    onClick={() => selectTab("groups")}
                                    className="rounded-full border border-border-strong/40 bg-bg-sidebar px-2.5 py-1 text-[11px] text-text-base transition-colors hover:border-border-strong"
                                  >
                                    {groupName}
                                  </button>
                                ))}
                              </div>
                            )}
                          </SummarySidebarSection>

                          <SummarySidebarSection title="Docs">
                            {recentDocsLinks.length === 0 ? (
                              <p className="text-[12px] text-text-muted">No docs links added yet.</p>
                            ) : (
                              <div className="space-y-2">
                                {recentDocsLinks.map(([key, entry]) => (
                                  <button
                                    key={key}
                                    onClick={() => handleExternalLinkClick(entry.path)}
                                    className="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-bg-sidebar"
                                  >
                                    <ExternalLink size={11} className="mt-0.5 shrink-0 text-text-muted" />
                                    <span className="min-w-0 text-[12px] text-text-base truncate">{entry.summary || key}</span>
                                  </button>
                                ))}
                              </div>
                            )}
                          </SummarySidebarSection>

                          <SummarySidebarSection title="Build">
                            {loadingBuildItems ? (
                              <p className="text-[12px] text-text-muted">Loading build items…</p>
                            ) : buildItems.length === 0 ? (
                              <p className="text-[12px] text-text-muted">No build items yet.</p>
                            ) : (
                              <div className="space-y-2">
                                {buildItems.map((item) => (
                                  <button
                                    key={item.id}
                                    onClick={() => selectTab("features")}
                                    className="flex w-full items-start justify-between gap-3 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-bg-sidebar"
                                  >
                                    <div className="min-w-0">
                                      <div className="truncate text-[12px] font-medium text-text-base">{item.title}</div>
                                      <div className="mt-0.5 text-[11px] text-text-muted">{relativeTime(item.updated_at)}</div>
                                    </div>
                                    <span className="shrink-0 rounded-full bg-bg-sidebar px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-text-muted">
                                      {item.state.replace(/_/g, " ")}
                                    </span>
                                  </button>
                                ))}
                              </div>
                            )}
                          </SummarySidebarSection>

                          <SummarySidebarSection title="Memory">
                            {loadingMemories ? (
                              <p className="text-[12px] text-text-muted">Loading memory…</p>
                            ) : (
                              <>
                                <div className="flex items-end gap-2">
                                  <span className="text-[24px] font-semibold leading-none tabular-nums text-text-base">{memoryCount}</span>
                                  <span className="pb-0.5 text-[12px] text-text-muted">{memoryCount === 1 ? "memory" : "memories"}</span>
                                </div>
                                <p className="text-[12px] text-text-muted">
                                  {memoryCount === 0 ? "No stored memories for this project yet." : "Stored memories are available for connected agents."}
                                </p>
                                <button
                                  onClick={() => selectTab("memory")}
                                  className="text-[11px] font-medium text-text-muted transition-colors hover:text-text-base"
                                >
                                  View memory
                                </button>
                              </>
                            )}
                          </SummarySidebarSection>
                        </div>

                      </div>
                    </div>
                  );
                })()}



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
                        agentOptions={project.agent_options}
                        onOptionChange={(agentId, patch) => {
                          const current = project.agent_options?.[agentId] ?? { claude_rules_in_dot_claude: true };
                          setProject({
                            ...project,
                            agent_options: {
                              ...(project.agent_options ?? {}),
                              [agentId]: { ...current, ...patch },
                            },
                            updated_at: new Date().toISOString(),
                          });
                          setDirty(true);
                        }}
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
                          showRemoveButtonAlways
                          emptyMessage="No skills attached."
                          onReadSkill={async (skillName) => {
                            const content: string = await invoke("read_skill", { name: skillName });
                            return content;
                          }}
                          onNavigateToSkill={onNavigateToSkill}
                          onForkSkill={async (skillName, content) => {
                            if (!selectedName) return;
                            try {
                              // A project directory is required so the skill file has
                              // somewhere to live. Give a clear error rather than letting
                              // the backend fail with a cryptic message.
                              if (!project.directory) {
                                throw new Error("Set a project directory before forking a skill to local.");
                              }

                              // Derive a unique local name: "<name>-copy", then
                              // "<name>-copy-2", "<name>-copy-3", … until we find one
                              // that isn't already in local_skills or global skills.
                              const taken = new Set([...project.skills, ...project.local_skills]);
                              let copyName = `${skillName}-copy`;
                              let n = 2;
                              while (taken.has(copyName)) {
                                copyName = `${skillName}-copy-${n}`;
                                n++;
                              }

                              // save_local_skill reads the project from disk, so flush
                              // current state first so agents/directory are up to date.
                              const toSave = {
                                ...project,
                                name: selectedName,
                                updated_at: new Date().toISOString(),
                              };
                              await invoke("save_project", {
                                name: selectedName,
                                data: JSON.stringify(toSave, null, 2),
                              });
                              setDirty(false);

                              // Write the skill content under the copy name into the
                              // project's local skill directory.
                              await invoke("save_local_skill", {
                                name: selectedName,
                                skillName: copyName,
                                content,
                              });

                              // Add the copy to local_skills; the original global skill
                              // is left untouched in project.skills.
                              const forkedProject = {
                                ...project,
                                name: selectedName,
                                local_skills: [...project.local_skills, copyName],
                                updated_at: new Date().toISOString(),
                              };
                              await invoke("save_project", {
                                name: selectedName,
                                data: JSON.stringify(forkedProject, null, 2),
                              });
                              setProject(forkedProject);
                              setDirty(false);
                              notifyProjectUpdated();
                              setSyncStatus(`Forked "${skillName}" → local skill "${copyName}"`);
                              setTimeout(() => setSyncStatus(null), 5000);
                            } catch (err: any) {
                              setError(`Fork failed: ${err}`);
                            }
                          }}
                         />
                      </section>

                     {/* ── AI skill suggestions ──────────────────────────── */}
                     <section>
                       <div className="flex items-center gap-2">
                         <Sparkles size={12} className="text-text-muted" />
                         <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">AI Suggestions</span>
                         {aiSkillsSuggestions.length > 0 && !aiSkillsLoading && (
                           <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-brand/10 text-brand border border-brand/20 leading-none">
                             {aiSkillsSuggestions.length}
                           </span>
                         )}
                         <div className="flex-1" />
                         <button
                           onClick={handleSuggestSkills}
                           disabled={aiSkillsLoading}
                           className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded-md disabled:opacity-40 transition-colors"
                           title="Ask AI to suggest skills based on this project's configuration"
                         >
                           <Sparkles size={11} className={aiSkillsLoading ? "animate-pulse" : ""} />
                           {aiSkillsLoading ? "Analysing…" : "Suggest skills"}
                         </button>
                       </div>

                       {aiSkillsLoading && (
                         <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg px-4 py-4 flex items-center gap-3">
                           <RefreshCw size={13} className="text-brand animate-spin flex-shrink-0" />
                           <p className="text-[12px] text-text-muted">Searching the skill library and marketplace…</p>
                         </div>
                       )}

                       {!aiSkillsLoading && aiSkillsSuggestions.length === 0 && (
                         <p className="mt-1.5 text-[12px] text-text-muted">
                           Click "Suggest skills" to get AI-powered recommendations based on this project.
                         </p>
                       )}

                       {!aiSkillsLoading && aiSkillsSuggestions.length > 0 && (
                         <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                           {aiSkillsSuggestions.map((rec) => (
                             <div key={rec.id} className="flex items-start gap-3 px-4 py-3 group hover:bg-surface-hover transition-colors">
                               <Sparkles size={13} className="flex-shrink-0 mt-0.5 text-brand" />
                               <div className="flex-1 min-w-0">
                                 <div className="flex items-center gap-2 mb-0.5">
                                   <span className="text-[13px] font-semibold text-text-base font-mono">{rec.title}</span>
                                   {rec.priority === "high" && (
                                     <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/20 leading-none">High</span>
                                   )}
                                 </div>
                                 <p className="text-[12px] text-text-muted leading-relaxed">{rec.body}</p>
                                  <div className="flex items-center gap-2 mt-2">
                                    {(onNavigateToSkillStoreWithResult || onNavigateToSkillStore) && (
                                      <button
                                        onClick={() => {
                                          // If the recommendation has full metadata (id, name, source, installs)
                                          // from the AI search result, use it to deep-link directly to the skill.
                                          if (rec.metadata && onNavigateToSkillStoreWithResult) {
                                            try {
                                              const meta = JSON.parse(rec.metadata) as { id: string; name: string; source: string; installs: number };
                                              if (meta.id && meta.name && meta.source) {
                                                onNavigateToSkillStoreWithResult(meta);
                                                return;
                                              }
                                            } catch {
                                              // fall through to plain query
                                            }
                                          }
                                          onNavigateToSkillStore?.(rec.title);
                                        }}
                                        className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                                      >
                                        <Search size={10} /> View
                                      </button>
                                    )}
                                     <SkillAddButton
                                      rec={rec}
                                      alreadyAdded={project.skills.includes(rec.title)}
                                      onAdd={async (skillName) => {
                                        const added = await addItem("skills", skillName);
                                        if (!added) return false;
                                        try {
                                          await invoke("action_recommendation", { id: rec.id });
                                          removeRecommendation(rec.id);
                                        } catch (err) {
                                          console.error("Failed to mark recommendation as actioned:", err);
                                        }
                                        return true;
                                      }}
                                    />
                                 </div>
                               </div>
                                <button
                                  onClick={async () => {
                                    await invoke("dismiss_recommendation", { id: rec.id });
                                    removeRecommendation(rec.id);
                                  }}
                                  className="flex-shrink-0 p-1 text-text-muted hover:text-text-base transition-colors opacity-0 group-hover:opacity-100"
                                  title="Dismiss"
                                >
                                  <X size={12} />
                                </button>
                             </div>
                           ))}
                         </div>
                       )}
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
                                  <TokenPill text={localSkillContentCache[s] ?? ""} />
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
                                <div className="flex items-center gap-1 ml-2">
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
                  // Agents that cannot have MCP config written by Automatic (e.g. Warp, Goose).
                  const noMcpAgents = availableAgents.filter(
                    (a) => project.agents.includes(a.id) && a.mcp_note
                  );
                  const allNoMcp = noMcpAgents.length > 0 && noMcpAgents.length === project.agents.length;
                  const someNoMcp = noMcpAgents.length > 0 && !allNoMcp;

                   return (
                  <section>
                    {/* All agents require manual MCP setup */}
                    {allNoMcp && (
                      <div className="mb-4 flex items-start gap-3 px-4 py-3 bg-bg-input border border-border-strong rounded-lg">
                        <AlertCircle size={15} className="text-text-muted flex-shrink-0 mt-0.5" />
                        <div>
                          <p className="text-[13px] font-medium text-text-base mb-0.5">MCP not configurable via Automatic</p>
                          {noMcpAgents.map((a) => (
                            <p key={a.id} className="text-[12px] text-text-muted leading-relaxed">{a.mcp_note}</p>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Some agents require manual MCP setup */}
                    {someNoMcp && (
                      <div className="mb-4 flex items-start gap-3 px-4 py-3 bg-warning/8 border border-warning/30 rounded-lg">
                        <AlertCircle size={15} className="text-warning flex-shrink-0 mt-0.5" />
                        <div>
                          <p className="text-[13px] font-medium text-warning mb-0.5">Some agents require manual MCP setup</p>
                          {noMcpAgents.map((a) => (
                            <p key={a.id} className="text-[12px] text-warning/80 leading-relaxed">
                              <span className="font-medium">{a.label}:</span> {a.mcp_note}
                            </p>
                          ))}
                        </div>
                      </div>
                    )}

                    <McpSelector
                      servers={project.mcp_servers}
                      availableServers={availableMcpServers}
                      onAdd={(s) => addItem("mcp_servers", s)}
                      onRemove={(i) => removeItem("mcp_servers", i)}
                      isServerEnabled={isMcpServerEnabled}
                      onToggleEnabled={toggleMcpServerEnabled}
                      showRemoveButtonAlways
                      disableAdd={allNoMcp}
                      emptyMessage={allNoMcp ? "Add other agent tools to enable MCP server syncing." : "No MCP servers attached."}
                      onNavigateToMcpServer={onNavigateToMcpServer}
                    />

                    {/* ── AI MCP suggestions ─────────────────────────────── */}
                    {!allNoMcp && (
                      <div className="mt-4 space-y-2">
                        <div className="flex items-center gap-2">
                          <Sparkles size={12} className="text-text-muted" />
                          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">AI Suggestions</span>
                          {aiMcpSuggestions.length > 0 && !aiMcpLoading && (
                            <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-brand/10 text-brand border border-brand/20 leading-none">
                              {aiMcpSuggestions.length}
                            </span>
                          )}
                          <div className="flex-1" />
                          <button
                            onClick={handleSuggestMcpServers}
                            disabled={aiMcpLoading}
                            className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded-md disabled:opacity-40 transition-colors"
                            title="Ask AI to suggest MCP servers based on this project's configuration"
                          >
                            <Sparkles size={11} className={aiMcpLoading ? "animate-pulse" : ""} />
                            {aiMcpLoading ? "Analysing…" : "Suggest MCP servers"}
                          </button>
                        </div>

                        {aiMcpLoading && (
                          <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-4 flex items-center gap-3">
                            <RefreshCw size={13} className="text-brand animate-spin flex-shrink-0" />
                            <p className="text-[12px] text-text-muted">Searching the MCP server catalogue…</p>
                          </div>
                        )}

                        {!aiMcpLoading && aiMcpSuggestions.length === 0 && (
                          <p className="text-[12px] text-text-muted">
                            Click "Suggest MCP servers" to get AI-powered recommendations based on this project.
                          </p>
                        )}

                        {!aiMcpLoading && aiMcpSuggestions.length > 0 && (
                          <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                            {aiMcpSuggestions.map((rec) => (
                              <div key={rec.id} className="flex items-start gap-3 px-4 py-3 group hover:bg-surface-hover transition-colors">
                                <Sparkles size={13} className="flex-shrink-0 mt-0.5 text-brand" />
                                <div className="flex-1 min-w-0">
                                  <div className="flex items-center gap-2 mb-0.5">
                                    <span className="text-[13px] font-semibold text-text-base font-mono">{rec.title}</span>
                                    {rec.priority === "high" && (
                                      <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/20 leading-none">High</span>
                                    )}
                                  </div>
                                  <p className="text-[12px] text-text-muted leading-relaxed">{rec.body}</p>
                                  <div className="flex items-center gap-2 mt-2">
                                    {onNavigateToMcpMarketplace && (
                                      <button
                                        onClick={() => onNavigateToMcpMarketplace(rec.title)}
                                        className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                                      >
                                        <Search size={10} /> View
                                      </button>
                                    )}
                                      <McpAddButton
                                       rec={rec}
                                       alreadyAdded={project.mcp_servers.includes(rec.title)}
                                       onAdd={async (serverName) => {
                                          const added = await addItem("mcp_servers", serverName);
                                          if (!added) return false;
                                          try {
                                            await invoke("action_recommendation", { id: rec.id });
                                            removeRecommendation(rec.id);
                                          } catch (err) {
                                            console.error("Failed to mark recommendation as actioned:", err);
                                          }
                                          return true;
                                        }}
                                      />
                                  </div>
                                </div>
                                 <button
                                   onClick={async () => {
                                     await invoke("dismiss_recommendation", { id: rec.id });
                                     removeRecommendation(rec.id);
                                   }}
                                   className="flex-shrink-0 p-1 text-text-muted hover:text-text-base transition-colors opacity-0 group-hover:opacity-100"
                                   title="Dismiss"
                                 >
                                   <X size={12} />
                                 </button>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </section>
                  );
                })()}


                {/* ── Documentation: Files & Dirs tab ─────────────── */}
                 {projectTab === "docs_files" && (
                   <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
                     {!project?.directory ? (
                       <div className="flex-1 flex items-center justify-center">
                         <p className="text-[13px] text-text-muted italic">
                           Set a project directory to use documentation.
                         </p>
                       </div>
                     ) : (
                       <div className="space-y-4">
                         {/* Header row */}
                         <div className="flex items-start justify-between gap-4">
                           <div className="min-w-0">
                             <h3 className="text-[13px] font-semibold text-text-base">Files &amp; Directories</h3>
                             <p className="mt-1 text-[12px] text-text-muted max-w-[820px]">
                               Add local folders, specs, or standalone files to include as project documentation. Stored in <code className="font-mono text-[11px]">.automatic/docs.json</code> and surfaced to agents via MCP.
                             </p>
                           </div>
                           {!showDocPathForm && (
                             <button
                               onClick={() => {
                                 setShowDocPathForm(true);
                                 setTimeout(() => {
                                   const input = document.getElementById("docs-path-input") as HTMLInputElement | null;
                                   input?.focus();
                                 }, 50);
                               }}
                               className="shrink-0 flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded-md shadow-sm transition-colors"
                             >
                               <Plus size={12} /> Add
                             </button>
                           )}
                         </div>

                         {/* Collapsible add form */}
                         {showDocPathForm && (
                           <div className="rounded-lg border border-brand/30 bg-bg-input overflow-hidden">
                             <div className="flex items-center justify-between px-3 py-2 border-b border-border-strong/20">
                               <span className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">Add path</span>
                               <button
                                 onClick={() => {
                                   setShowDocPathForm(false);
                                   setDocNewPath("");
                                   setDocNewPathSummary("");
                                 }}
                                 className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                               >
                                 Cancel
                               </button>
                             </div>
                             <div className="px-3 py-3">
                               <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_180px]">
                                 <div className="space-y-3 min-w-0">
                                   <div>
                                     <label className="mb-1.5 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">Path</label>
                                     <input
                                       id="docs-path-input"
                                       type="text"
                                       value={docNewPath}
                                       onChange={(e) => setDocNewPath(e.target.value)}
                                       onKeyDown={(e) => {
                                         if (e.key === "Enter") { e.preventDefault(); void handleAddDocPath(); }
                                         if (e.key === "Escape") { setShowDocPathForm(false); setDocNewPath(""); setDocNewPathSummary(""); }
                                       }}
                                       placeholder="/path/to/specs or ./docs/architecture.md"
                                       className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] text-text-base placeholder-text-muted outline-none transition-colors focus:border-brand/60"
                                     />
                                   </div>
                                   <div>
                                     <label className="mb-1.5 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">Description</label>
                                     <input
                                       type="text"
                                       value={docNewPathSummary}
                                       onChange={(e) => setDocNewPathSummary(e.target.value)}
                                       onKeyDown={(e) => {
                                         if (e.key === "Enter") { e.preventDefault(); void handleAddDocPath(); }
                                         if (e.key === "Escape") { setShowDocPathForm(false); setDocNewPath(""); setDocNewPathSummary(""); }
                                       }}
                                       placeholder="What should agents use this for?"
                                       className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] text-text-base placeholder-text-muted outline-none transition-colors focus:border-brand/60"
                                     />
                                   </div>
                                 </div>
                                 <div className="flex flex-col gap-2 justify-end">
                                   <button
                                     onClick={handleBrowseDocPath}
                                     className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] font-medium text-text-muted transition-colors hover:bg-surface-hover hover:text-text-base flex items-center justify-center gap-1.5"
                                     title="Pick a directory"
                                   >
                                     <FolderOpen size={12} /> Browse
                                   </button>
                                   <button
                                     onClick={handleAddDocPath}
                                     disabled={!docNewPath.trim()}
                                     className="w-full inline-flex items-center justify-center gap-1.5 rounded-md bg-brand px-3 py-2 text-[12px] font-medium text-white shadow-sm transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
                                   >
                                     <Plus size={12} /> Add path
                                   </button>
                                 </div>
                               </div>
                             </div>
                           </div>
                         )}

                         {/* Paths list */}
                         {fileDocEntries.length === 0 ? (
                           <div className="rounded-lg border border-dashed border-border-strong/40 px-4 py-10 text-center">
                             <div className="mx-auto mb-3 flex h-11 w-11 items-center justify-center rounded-full border border-dashed border-border-strong/50 bg-bg-sidebar/50">
                               <FolderPlus size={16} className="text-text-muted" />
                             </div>
                             <h5 className="text-[13px] font-medium text-text-base">No documentation paths yet</h5>
                             <p className="mx-auto mt-1 max-w-[420px] text-[12px] leading-relaxed text-text-muted">
                               Add architecture docs, spec folders, generated references, or any local files agents should read alongside the project.
                             </p>
                           </div>
                         ) : (
                           <div className="rounded-lg border border-border-strong/40 overflow-hidden">
                             <div className="flex items-center justify-between gap-3 px-3 py-2.5 border-b border-border-strong/20 bg-bg-input">
                               <span className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">Included paths</span>
                               <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-text-muted">
                                 {fileDocEntries.length}
                               </span>
                             </div>
                             <div className="divide-y divide-border-strong/20">
                               {fileDocEntries.map(([key, entry]) => {
                                 const relativePath = getProjectRelativeDocPath(project.directory, entry.path);
                                 const displayPath = relativePath
                                   ? relativePath === "." ? project.directory : `./${relativePath}`
                                   : entry.path;
                                 return (
                                   <div key={key} className="group flex items-center gap-3 px-3 py-2.5 transition-colors hover:bg-surface-hover/50">
                                     <div className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md border border-border-strong/30 bg-bg-sidebar/60">
                                       <FolderOpen size={13} className="text-text-muted" />
                                     </div>
                                     <div className="min-w-0 flex-1">
                                       <div className="flex flex-wrap items-baseline gap-2">
                                         <span className="text-[12px] font-medium text-text-base truncate">{entry.summary || key}</span>
                                         <span className="shrink-0 rounded-full border border-border-strong/30 bg-bg-sidebar px-1.5 py-px text-[10px] text-text-muted">
                                           {relativePath ? "In project" : "Absolute path"}
                                         </span>
                                       </div>
                                       <p className="mt-0.5 font-mono text-[11px] text-text-muted/80 truncate" title={displayPath}>
                                         {displayPath}
                                       </p>
                                     </div>
                                     <button
                                       onClick={() => removeDocEntry(key, false)}
                                       className="opacity-0 group-hover:opacity-100 rounded-md border border-transparent p-1.5 text-text-muted transition-all hover:border-error/20 hover:bg-error/10 hover:text-error"
                                       title="Remove path"
                                     >
                                       <Trash2 size={13} />
                                     </button>
                                   </div>
                                 );
                               })}
                             </div>
                           </div>
                         )}
                       </div>
                     )}
                   </div>
                 )}

                 {/* ── Documentation: Links tab ─────────────────────── */}
                 {projectTab === "docs_links" && (
                   <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
                     {!project?.directory ? (
                       <div className="flex-1 flex items-center justify-center">
                         <p className="text-[13px] text-text-muted italic">
                           Set a project directory to use documentation.
                         </p>
                       </div>
                     ) : (
                       <div className="space-y-4">
                         {/* Header row */}
                         <div className="flex items-start justify-between gap-4">
                           <div className="min-w-0">
                             <h3 className="text-[13px] font-semibold text-text-base">Links</h3>
                             <p className="mt-1 text-[12px] text-text-muted max-w-[820px]">
                               Add URLs to external documentation, design specs, or reference material so this project keeps its key web resources in one place.
                             </p>
                           </div>
                           {!showDocLinkForm && (
                             <button
                               onClick={() => {
                                 setShowDocLinkForm(true);
                                 setTimeout(() => {
                                   const input = document.getElementById("docs-link-input") as HTMLInputElement | null;
                                   input?.focus();
                                 }, 50);
                               }}
                               className="shrink-0 flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded-md shadow-sm transition-colors"
                             >
                               <Plus size={12} /> Add
                             </button>
                           )}
                         </div>

                         {/* Collapsible add form */}
                         {showDocLinkForm && (
                           <div className="rounded-lg border border-brand/30 bg-bg-input overflow-hidden">
                             <div className="flex items-center justify-between px-3 py-2 border-b border-border-strong/20">
                               <span className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">Add link</span>
                               <button
                                 onClick={() => {
                                   setShowDocLinkForm(false);
                                   setDocNewLinkUrl("");
                                   setDocNewLinkLabel("");
                                 }}
                                 className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                               >
                                 Cancel
                               </button>
                             </div>
                             <div className="px-3 py-3">
                               <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_220px_120px]">
                                 <div>
                                   <label className="mb-1.5 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">URL</label>
                                   <input
                                     id="docs-link-input"
                                     type="url"
                                     value={docNewLinkUrl}
                                     onChange={(e) => setDocNewLinkUrl(e.target.value)}
                                     onKeyDown={(e) => {
                                       if (e.key === "Enter") { e.preventDefault(); void handleAddDocLink(); }
                                       if (e.key === "Escape") { setShowDocLinkForm(false); setDocNewLinkUrl(""); setDocNewLinkLabel(""); }
                                     }}
                                     placeholder="https://docs.example.com/reference"
                                     className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] text-text-base placeholder-text-muted outline-none transition-colors focus:border-brand/60"
                                   />
                                 </div>
                                 <div>
                                   <label className="mb-1.5 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">Label</label>
                                   <input
                                     type="text"
                                     value={docNewLinkLabel}
                                     onChange={(e) => setDocNewLinkLabel(e.target.value)}
                                     onKeyDown={(e) => {
                                       if (e.key === "Enter") { e.preventDefault(); void handleAddDocLink(); }
                                       if (e.key === "Escape") { setShowDocLinkForm(false); setDocNewLinkUrl(""); setDocNewLinkLabel(""); }
                                     }}
                                     placeholder="What is this link useful for?"
                                     className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] text-text-base placeholder-text-muted outline-none transition-colors focus:border-brand/60"
                                   />
                                 </div>
                                 <div className="flex items-end">
                                   <button
                                     onClick={handleAddDocLink}
                                     disabled={!docNewLinkUrl.trim()}
                                     className="w-full inline-flex items-center justify-center gap-1.5 rounded-md bg-brand px-3 py-2 text-[12px] font-medium text-white shadow-sm transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
                                   >
                                     <Plus size={12} /> Add link
                                   </button>
                                 </div>
                               </div>
                             </div>
                           </div>
                         )}

                         {/* Links list */}
                         {linkDocEntries.length === 0 ? (
                           <div className="rounded-lg border border-dashed border-border-strong/40 px-4 py-10 text-center">
                             <div className="mx-auto mb-3 flex h-11 w-11 items-center justify-center rounded-full border border-dashed border-border-strong/50 bg-bg-sidebar/50">
                               <LinkIcon size={16} className="text-text-muted" />
                             </div>
                             <h5 className="text-[13px] font-medium text-text-base">No external references yet</h5>
                             <p className="mx-auto mt-1 max-w-[420px] text-[12px] leading-relaxed text-text-muted">
                               Add product docs, API references, Figma files, tickets, or other URLs that help explain how this project works.
                             </p>
                           </div>
                         ) : (
                           <div className="rounded-lg border border-border-strong/40 overflow-hidden">
                             <div className="flex items-center justify-between gap-3 px-3 py-2.5 border-b border-border-strong/20 bg-bg-input">
                               <span className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">Saved links</span>
                               <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-text-muted">
                                 {linkDocEntries.length}
                               </span>
                             </div>
                             <div className="divide-y divide-border-strong/20">
                               {linkDocEntries.map(([key, entry]) => (
                                 <div key={key} className="group flex items-center gap-3 px-3 py-2.5 transition-colors hover:bg-surface-hover/50">
                                   <div className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md border border-border-strong/30 bg-bg-sidebar/60">
                                     <Globe size={13} className="text-text-muted" />
                                   </div>
                                   <div className="min-w-0 flex-1">
                                     <div className="flex flex-wrap items-baseline gap-2">
                                       <span className="text-[12px] font-medium text-text-base truncate">{entry.summary || key}</span>
                                       <span className="shrink-0 rounded-full border border-border-strong/30 bg-bg-sidebar px-1.5 py-px text-[10px] text-text-muted">external</span>
                                     </div>
                                     <p className="mt-0.5 font-mono text-[11px] text-text-muted/80 truncate" title={entry.path}>
                                       {entry.path}
                                     </p>
                                   </div>
                                   <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                      <a
                                        href={entry.path}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        onClick={handleExternalLinkClick(entry.path, true)}
                                        className="rounded-md border border-border-strong/30 p-1.5 text-text-muted transition-colors hover:border-brand/30 hover:bg-brand/10 hover:text-brand"
                                        title="Open link"
                                      >
                                       <ExternalLink size={13} />
                                     </a>
                                     <button
                                       onClick={() => removeDocEntry(key, false)}
                                       className="rounded-md border border-transparent p-1.5 text-text-muted transition-colors hover:border-error/20 hover:bg-error/10 hover:text-error"
                                       title="Remove link"
                                     >
                                       <Trash2 size={13} />
                                     </button>
                                   </div>
                                 </div>
                               ))}
                             </div>
                           </div>
                         )}
                       </div>
                     )}
                   </div>
                 )}

                {/* ── Documentation: Notes tab ─────────────────────── */}
                {projectTab === "docs_notes" && (
                  <div className="flex-1 flex min-h-0">
                    {!project?.directory ? (
                      <div className="flex-1 flex items-center justify-center">
                        <p className="text-[13px] text-text-muted italic">
                          Set a project directory to use documentation.
                        </p>
                      </div>
                    ) : (
                      <>
                        {/* Left panel: note list */}
                        <div className="w-52 flex-shrink-0 border-r border-border-strong/40 flex flex-col min-h-0">
                          <div className="px-3 py-2.5 border-b border-border-strong/40 flex items-center justify-between flex-shrink-0">
                            <span className="text-[11px] font-semibold text-text-muted uppercase tracking-wide">Notes</span>
                            <button
                              onClick={() => setDocNewNoteCreating(true)}
                              className="p-0.5 rounded text-text-muted hover:text-brand hover:bg-brand/10 transition-colors"
                              title="New note"
                            >
                              <Plus size={13} />
                            </button>
                          </div>

                          {/* New note name input */}
                          {docNewNoteCreating && (
                            <div className="px-2 py-2 border-b border-border-strong/20 flex items-center gap-1">
                              <input
                                autoFocus
                                type="text"
                                value={docNewNoteName}
                                onChange={(e) => setDocNewNoteName(e.target.value)}
                                onKeyDown={async (e) => {
                                  if (e.key === "Enter") await createDocNote(docNewNoteName);
                                  if (e.key === "Escape") {
                                    setDocNewNoteCreating(false);
                                    setDocNewNoteName("");
                                  }
                                }}
                                placeholder="Note name…"
                                className="flex-1 px-2 py-1 text-[11px] bg-bg-input border border-brand/60 rounded text-text-base placeholder-text-muted focus:outline-none"
                              />
                              <button
                                onClick={() => createDocNote(docNewNoteName)}
                                disabled={!docNewNoteName.trim()}
                                className="p-1 rounded text-brand hover:bg-brand/10 transition-colors disabled:opacity-40"
                              >
                                <Check size={11} />
                              </button>
                              <button
                                onClick={() => { setDocNewNoteCreating(false); setDocNewNoteName(""); }}
                                className="p-1 rounded text-text-muted hover:text-error hover:bg-error/10 transition-colors"
                              >
                                <X size={11} />
                              </button>
                            </div>
                          )}

                          {/* Note list */}
                          <div className="flex-1 overflow-y-auto">
                            {(() => {
                              const docs = parsedDocs();
                              const noteEntries = Object.entries(docs).filter(
                                ([, v]) => v.path.startsWith(".automatic/docs/")
                              );
                              if (noteEntries.length === 0) {
                                return (
                                  <p className="px-3 py-4 text-[11px] text-text-muted italic">
                                    No notes yet. Click + to create one.
                                  </p>
                                );
                              }
                              return noteEntries.map(([key, entry]) => (
                                <div
                                  key={key}
                                  className={`group flex items-center gap-2 px-3 py-2 cursor-pointer transition-colors ${
                                    docNoteSelected === key
                                      ? "bg-brand/10 text-text-base"
                                      : "hover:bg-surface-hover text-text-muted hover:text-text-base"
                                  }`}
                                  onClick={() => {
                                    if (docNoteSelected !== key) loadDocNote(key);
                                  }}
                                >
                                  <FileText size={12} className="flex-shrink-0" />
                                  <span className="flex-1 text-[12px] truncate">{entry.summary || key}</span>
                                  <button
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      removeDocEntry(key, true);
                                    }}
                                    className="opacity-0 group-hover:opacity-100 p-0.5 rounded text-text-muted hover:text-error hover:bg-error/10 transition-all"
                                    title="Delete note"
                                  >
                                    <Trash2 size={10} />
                                  </button>
                                </div>
                              ));
                            })()}
                          </div>
                        </div>

                        {/* Right panel: editor */}
                        <div className="flex-1 flex flex-col min-h-0">
                          {docNoteSelected === null ? (
                            <div className="flex-1 flex items-center justify-center text-center p-8">
                              <div>
                                <FileText size={28} className="mx-auto mb-3 text-text-muted opacity-40" strokeWidth={1.5} />
                                <p className="text-[13px] text-text-muted">Select a note to edit, or create a new one.</p>
                              </div>
                            </div>
                          ) : docNoteLoading ? (
                            <div className="flex-1 flex items-center justify-center text-text-muted">
                              <RefreshCw size={14} className="animate-spin mr-2" />
                              <span className="text-[13px]">Loading…</span>
                            </div>
                          ) : (
                            <>
                              {/* Note toolbar */}
                              <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
                                <span className="text-[11px] text-text-muted font-mono">
                                  .automatic/docs/{docNoteSelected}.md
                                  {docNoteDirty ? " (unsaved)" : ""}
                                </span>
                                <button
                                  onClick={saveDocNote}
                                  disabled={!docNoteDirty || docNoteSaving}
                                  className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                                >
                                  <Check size={10} /> {docNoteSaving ? "Saving…" : "Save"}
                                </button>
                              </div>
                              {/* Markdown textarea */}
                              <textarea
                                value={docNoteContent}
                                onChange={(e) => {
                                  setDocNoteContent(e.target.value);
                                  setDocNoteDirty(true);
                                }}
                                spellCheck={false}
                                className="flex-1 p-4 text-[13px] font-mono text-text-base bg-bg-base resize-none focus:outline-none leading-relaxed"
                                placeholder="Write Markdown here…"
                              />
                            </>
                          )}
                        </div>
                      </>
                    )}
                  </div>
                )}

                {/* ── Memory tab ──────────────────────────────────── */}
                {projectTab === "memory" && selectedName && (
                  <>
                    <MemoryBrowser
                      projectName={selectedName}
                      memories={memories}
                      loading={loadingMemories}
                      onRefresh={() => loadMemories(selectedName)}
                      onError={(msg) => setError(msg)}
                    />
                    {project?.directory && project.agents.includes("claude") && (
                      <ClaudeMemoryPanel
                        projectName={selectedName}
                        projectDirectory={project.directory}
                        onPromoted={() => loadMemories(selectedName)}
                      />
                    )}
                  </>
                )}

                {/* ── Activity tab ─────────────────────────────────── */}
                {projectTab === "activity" && selectedName && (() => {
                  const totalPages = Math.max(1, Math.ceil(activityTotalCount / ACTIVITY_PAGE_SIZE));
                  return (
                    <section className="flex flex-col gap-0">
                      {/* Header row */}
                      <div className="flex items-center justify-between px-1 pb-3 flex-shrink-0">
                        <div className="flex items-center gap-2">
                          <History size={13} className="text-text-muted" />
                          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Activity Log</span>
                          {activityTotalCount > 0 && (
                            <span className="text-[11px] text-text-muted">({activityTotalCount} total)</span>
                          )}
                        </div>
                        <button
                          onClick={() => loadActivityPage(selectedName, activityPage)}
                          disabled={loadingActivityPage}
                          className="text-[11px] text-text-muted hover:text-text-base transition-colors flex items-center gap-1 disabled:opacity-40"
                        >
                          <RefreshCw size={11} className={loadingActivityPage ? "animate-spin" : ""} />
                          Refresh
                        </button>
                      </div>

                      {/* Entries list */}
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
                        {loadingActivityPage ? (
                          <div className="px-4 py-8 text-center text-[12px] text-text-muted">
                            <RefreshCw size={14} className="animate-spin mx-auto mb-2" />
                            Loading activity…
                          </div>
                        ) : activityPageEntries.length === 0 ? (
                          <div className="px-4 py-8 text-center text-[12px] text-text-muted italic">
                            No activity recorded yet. Save or sync the project to start logging events.
                          </div>
                        ) : (
                          activityPageEntries.map((item, i) => {
                            const { icon, dot } = activityMeta(item.event);
                            return (
                              <div
                                key={item.id}
                                className={`flex items-center gap-3 px-4 py-2.5 ${i < activityPageEntries.length - 1 ? "border-b border-border-strong/20" : ""}`}
                              >
                                <div className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${dot}`} />
                                <div className="flex-shrink-0 text-text-muted">{icon}</div>
                                <div className="flex-1 min-w-0">
                                  <span className="text-[12px] text-text-base">{item.label}</span>
                                  {item.detail && (
                                    <span className="text-[12px] text-text-muted ml-1.5">{item.detail}</span>
                                  )}
                                </div>
                                <span className="text-[11px] text-text-muted flex-shrink-0">{relativeTime(item.timestamp)}</span>
                              </div>
                            );
                          })
                        )}
                      </div>

                      {/* Pagination controls */}
                      {totalPages > 1 && (
                        <div className="flex items-center justify-between pt-3 flex-shrink-0">
                          <button
                            onClick={() => loadActivityPage(selectedName, activityPage - 1)}
                            disabled={activityPage === 0 || loadingActivityPage}
                            className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium text-text-muted hover:text-text-base border border-border-strong/40 rounded-md disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
                          >
                            <ChevronLeft size={13} /> Previous
                          </button>
                          <span className="text-[12px] text-text-muted">
                            Page {activityPage + 1} of {totalPages}
                          </span>
                          <button
                            onClick={() => loadActivityPage(selectedName, activityPage + 1)}
                            disabled={activityPage >= totalPages - 1 || loadingActivityPage}
                            className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium text-text-muted hover:text-text-base border border-border-strong/40 rounded-md disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
                          >
                            Next <ChevronRight size={13} />
                          </button>
                        </div>
                      )}
                    </section>
                  );
                })()}

                {/* ── Recommendations tab ──────────────────────────── */}
                {projectTab === "recommendations" && (
                  <section className="space-y-3">
                    {/* Header row */}
                    <div className="flex items-center gap-2">
                      <Sparkles size={13} className="text-text-muted" />
                      <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Recommendations</span>
                      {recsDisplayCount > 0 && (
                        <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-warning/15 text-warning border border-warning/20 leading-none">
                          {recsDisplayCount}
                        </span>
                      )}
                      <div className="flex-1" />
                      {/* Last-run metadata */}
                      {aiRecsLastRunAt && !aiRecsLoading && (
                        <span className="text-[11px] text-text-muted">
                          Updated {relativeTime(aiRecsLastRunAt)}
                        </span>
                      )}
                      {/* Manual trigger button */}
                      <button
                        onClick={handleUpdateAiRecommendations}
                        disabled={aiRecsLoading}
                        className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded-md disabled:opacity-40 transition-colors"
                        title="Re-run AI analysis to refresh recommendations"
                      >
                        <RefreshCw size={11} className={aiRecsLoading ? "animate-spin" : ""} />
                        {aiRecsLoading ? "Analysing…" : "Update recommendations"}
                      </button>
                    </div>

                    {/* AI loading state */}
                    {aiRecsLoading && (
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-6 flex items-center gap-3">
                        <RefreshCw size={14} className="text-brand animate-spin flex-shrink-0" />
                        <div>
                          <p className="text-[13px] font-medium text-text-base">Analysing project…</p>
                          <p className="text-[12px] text-text-muted">The AI is reviewing your configuration and searching for relevant skills and MCP servers.</p>
                        </div>
                      </div>
                    )}

                    {!aiRecsLoading && recsDisplayCount === 0 ? (
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-10 text-center">
                        <Sparkles size={18} className="text-text-muted mx-auto mb-2" />
                        <p className="text-[13px] font-medium text-text-base mb-1">No recommendations at this time</p>
                        <p className="text-[12px] text-text-muted">Click "Update recommendations" to run an AI analysis of your project configuration.</p>
                      </div>
                    ) : !aiRecsLoading ? (
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                        {/* AI skill rollup card */}
                        {aiSkillsRollupCount > 0 && (
                          <div className="flex items-start gap-3 px-4 py-4 hover:bg-surface-hover transition-colors">
                            <Sparkles size={14} className="flex-shrink-0 mt-0.5 text-brand" />
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center gap-2 mb-1">
                                <span className="text-[13px] font-semibold text-text-base">
                                  {aiSkillsRollupCount} skill{aiSkillsRollupCount !== 1 ? "s" : ""} recommended
                                </span>
                                <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-brand/10 text-brand border border-brand/20 leading-none flex items-center gap-1">
                                  <Sparkles size={8} /> AI
                                </span>
                              </div>
                              <p className="text-[12px] text-text-muted leading-relaxed">
                                The AI has identified {aiSkillsRollupCount} skill{aiSkillsRollupCount !== 1 ? "s" : ""} that may benefit this project. Review and add them from the Skills tab.
                              </p>
                              <button
                                onClick={() => selectTab("skills")}
                                className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                              >
                                <Code size={10} /> Go to Skills tab <ArrowRight size={10} />
                              </button>
                            </div>
                          </div>
                        )}

                        {/* AI MCP rollup card */}
                        {aiMcpRollupCount > 0 && (
                          <div className="flex items-start gap-3 px-4 py-4 hover:bg-surface-hover transition-colors">
                            <Sparkles size={14} className="flex-shrink-0 mt-0.5 text-brand" />
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center gap-2 mb-1">
                                <span className="text-[13px] font-semibold text-text-base">
                                  {aiMcpRollupCount} MCP server{aiMcpRollupCount !== 1 ? "s" : ""} recommended
                                </span>
                                <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-brand/10 text-brand border border-brand/20 leading-none flex items-center gap-1">
                                  <Sparkles size={8} /> AI
                                </span>
                              </div>
                              <p className="text-[12px] text-text-muted leading-relaxed">
                                The AI has identified {aiMcpRollupCount} MCP server{aiMcpRollupCount !== 1 ? "s" : ""} that may benefit this project. Review and add them from the MCP Servers tab.
                              </p>
                              <button
                                onClick={() => selectTab("mcp_servers")}
                                className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                              >
                                <Server size={10} /> Go to MCP Servers tab <ArrowRight size={10} />
                              </button>
                            </div>
                          </div>
                        )}

                        {/* Normal (non-AI-suggestion) recommendation cards */}
                        {normalRecs.map((rec) => (
                          <div key={rec.id} className="flex items-start gap-3 px-4 py-4 group hover:bg-surface-hover transition-colors">
                            <AlertCircle
                              size={14}
                              className={`flex-shrink-0 mt-0.5 ${rec.priority === "high" ? "text-warning" : "text-text-muted"}`}
                            />
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center gap-2 mb-1">
                                <span className="text-[13px] font-semibold text-text-base">{rec.title}</span>
                                {rec.priority === "high" && (
                                  <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/20 leading-none">
                                    Important
                                  </span>
                                )}
                                {rec.source === "automatic-ai" && (
                                  <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-brand/10 text-brand border border-brand/20 leading-none flex items-center gap-1">
                                    <Sparkles size={8} />
                                    AI
                                  </span>
                                )}
                              </div>
                              <p className="text-[12px] text-text-muted leading-relaxed">{rec.body}</p>
                              {(rec.kind === "skill" || rec.kind === "mcp_server") && (
                                <div className="mt-2 flex items-center gap-2">
                                  {rec.kind === "skill" && (onNavigateToSkillStoreWithResult || onNavigateToSkillStore) && (
                                    <button
                                      onClick={() => {
                                        if (rec.metadata && onNavigateToSkillStoreWithResult) {
                                          try {
                                            const meta = JSON.parse(rec.metadata) as { id: string; name: string; source: string; installs: number };
                                            if (meta.id && meta.name && meta.source) {
                                              onNavigateToSkillStoreWithResult(meta);
                                              return;
                                            }
                                          } catch {
                                            // fall through to plain query
                                          }
                                        }
                                        onNavigateToSkillStore?.(rec.title);
                                      }}
                                      className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                                    >
                                      <Search size={10} /> View
                                    </button>
                                  )}
                                  {rec.kind === "skill" && (
                                    <SkillAddButton
                                      rec={rec}
                                      alreadyAdded={project.skills.includes(rec.title)}
                                      onAdd={async (skillName) => {
                                        const added = await addItem("skills", skillName);
                                        if (!added) return false;
                                        try {
                                          await invoke("action_recommendation", { id: rec.id });
                                          removeRecommendation(rec.id);
                                        } catch (err) {
                                          console.error("Failed to mark recommendation as actioned:", err);
                                        }
                                        return true;
                                      }}
                                    />
                                  )}

                                  {rec.kind === "mcp_server" && onNavigateToMcpMarketplace && (
                                    <button
                                      onClick={() => {
                                        if (rec.metadata) {
                                          try {
                                            const meta = JSON.parse(rec.metadata) as { slug?: string };
                                            if (meta.slug) {
                                              onNavigateToMcpMarketplace(meta.slug);
                                              return;
                                            }
                                          } catch {
                                            // fall through to title
                                          }
                                        }
                                        onNavigateToMcpMarketplace(rec.title);
                                      }}
                                      className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                                    >
                                      <Search size={10} /> View
                                    </button>
                                  )}
                                  {rec.kind === "mcp_server" && (
                                    <McpAddButton
                                      rec={rec}
                                      alreadyAdded={project.mcp_servers.includes(rec.title)}
                                      onAdd={async (serverName) => {
                                        const added = await addItem("mcp_servers", serverName);
                                        if (!added) return false;
                                        try {
                                          await invoke("action_recommendation", { id: rec.id });
                                          removeRecommendation(rec.id);
                                        } catch (err) {
                                          console.error("Failed to mark recommendation as actioned:", err);
                                        }
                                        return true;
                                      }}
                                    />
                                  )}
                                </div>
                              )}
                              {rec.kind === "rule" && (
                                <button
                                  onClick={() => selectTab("project_file")}
                                  className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                                >
                                  Open Project File <ArrowRight size={10} />
                                </button>
                              )}
                              {rec.kind === "project_file" && (
                                <button
                                  onClick={() => selectTab("project_file")}
                                  className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                                >
                                  Create Instructions File <ArrowRight size={10} />
                                </button>
                              )}
                            </div>
                            <button
                              onClick={() => handleDismissRecommendation(rec.id)}
                              className="flex-shrink-0 p-1 text-text-muted hover:text-text-base transition-colors opacity-0 group-hover:opacity-100"
                              title="Dismiss"
                            >
                              <X size={13} />
                            </button>
                          </div>
                        ))}
                      </div>
                    ) : null}
                  </section>
                )}

                {/* ── Groups tab ───────────────────────────────────── */}
                {projectTab === "groups" && selectedName && (
                  <section className="space-y-4">
                    {/* Header */}
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Layers size={13} className="text-text-muted" />
                        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Groups</span>
                        {projectGroupMemberships.length > 0 && (
                          <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-bg-sidebar text-text-muted border border-border-strong/30 leading-none">
                            {projectGroupMemberships.length}
                          </span>
                        )}
                      </div>
                      <button
                        onClick={() => loadGroups(selectedName)}
                        disabled={loadingGroups}
                        className="text-[11px] text-text-muted hover:text-text-base transition-colors flex items-center gap-1 disabled:opacity-40"
                        title="Refresh"
                      >
                        <RefreshCw size={11} className={loadingGroups ? "animate-spin" : ""} />
                        Refresh
                      </button>
                    </div>

                    {/* Current memberships */}
                    {loadingGroups ? (
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-6 flex items-center gap-2 text-text-muted text-[12px]">
                        <RefreshCw size={12} className="animate-spin" />
                        Loading groups…
                      </div>
                    ) : projectGroupMemberships.length === 0 ? (
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-8 text-center">
                        <Layers size={18} className="text-text-muted/40 mx-auto mb-2" />
                        <p className="text-[13px] font-medium text-text-base mb-1">Not in any group</p>
                        <p className="text-[12px] text-text-muted">
                          Add this project to a group to link it with related projects.
                          When synced, agents will see context about all projects in the group.
                        </p>
                      </div>
                    ) : (
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                        {projectGroupMemberships.map((groupName) => (
                          <div
                            key={groupName}
                            className="flex items-center gap-3 px-4 py-2.5 hover:bg-surface-hover transition-colors"
                          >
                            <Layers size={13} className="text-text-muted flex-shrink-0" />
                            <span className="flex-1 text-[13px] text-text-base">{groupName}</span>
                            <button
                              onClick={() => onNavigateToGroup?.(groupName)}
                              className="flex-shrink-0 px-2 py-0.5 text-[11px] font-medium text-brand hover:text-brand-hover hover:bg-brand/10 rounded transition-colors flex items-center gap-1"
                              title={`View ${groupName} group`}
                            >
                              View
                              <ExternalLink size={10} />
                            </button>
                            <button
                              onClick={() => handleRemoveFromGroup(groupName, selectedName)}
                              className="flex-shrink-0 p-1 text-text-muted hover:text-red-400 transition-colors"
                              title={`Remove from "${groupName}"`}
                            >
                              <X size={12} />
                            </button>
                          </div>
                        ))}
                      </div>
                    )}

                    {/* Remove from all groups button */}
                    {projectGroupMemberships.length > 1 && (
                      <button
                        onClick={() => handleRemoveFromAllGroups(selectedName)}
                        className="w-full flex items-center justify-center gap-2 px-4 py-2 text-[12px] text-red-400 hover:text-red-300 hover:bg-red-500/10 rounded-lg transition-colors"
                      >
                        <Trash2 size={12} />
                        Remove from all groups
                      </button>
                    )}

                    {/* Add to a group picker */}
                    {(() => {
                      const available = allGroups.filter((g) => !projectGroupMemberships.includes(g));
                      if (available.length === 0 && allGroups.length > 0 && !loadingGroups) return null;
                      return (
                        <div>
                          <p className="text-[11px] font-semibold text-text-muted uppercase tracking-wider mb-2">
                            {allGroups.length === 0 ? "No groups exist yet" : "Add to group"}
                          </p>
                          {allGroups.length === 0 ? (
                            <p className="text-[12px] text-text-muted">
                              Create groups from the <strong>Groups</strong> section in the sidebar to start linking related projects.
                            </p>
                          ) : available.length > 0 ? (
                            <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                              {available.map((groupName) => (
                                <button
                                  key={groupName}
                                  onClick={() => handleAddToGroup(groupName, selectedName)}
                                  className="w-full flex items-center gap-3 px-4 py-2.5 text-left hover:bg-surface-hover transition-colors"
                                >
                                  <Plus size={12} className="text-text-muted flex-shrink-0" />
                                  <span className="flex-1 text-[13px] text-text-muted">{groupName}</span>
                                </button>
                              ))}
                            </div>
                          ) : null}
                        </div>
                      );
                    })()}

                    {/* Callout */}
                    <div className="rounded-md bg-bg-input border border-border-strong/30 px-3 py-2.5 text-[12px] text-text-muted space-y-1">
                      <p className="font-medium text-text-base">How groups work</p>
                      <p>
                        When this project is synced, Automatic injects a context block into its agent instruction
                        files listing all related projects — with their descriptions and relative paths — so
                        agents can recognise and navigate between them.
                      </p>
                    </div>
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
              onClick={() => startCreate()}
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
        projectName={selectedName ?? undefined}
        onClose={() => setDriftDiffFile(null)}
        onResolved={handleDriftResolved}
      />
    )}

    {/* ── Instruction file conflict modal ──────────────────────────────── */}
    {instructionConflict && selectedName && (
      <InstructionConflictModal
        conflict={instructionConflict}
        projectName={selectedName}
        onAdopt={(adopted) => handleAdoptInstructionFile(instructionConflict.filename, adopted)}
        onOverwrite={() => handleOverwriteInstructionFile(instructionConflict.filename)}
        onClose={() => setInstructionConflict(null)}
      />
    )}

    {rebuildPreview && (
      <RebuildConfirmationModal
        preview={rebuildPreview}
        busy={rebuildBusy}
        onConfirm={confirmRebuild}
        onClose={() => {
          if (!rebuildBusy) {
            setRebuildPreview(null);
            setSyncStatus(null);
          }
        }}
      />
    )}

    {/* ── Drag ghost — follows the cursor while dragging a project or folder ─── */}
    {dragGhost && (
      <div
        style={{
          position: "fixed",
          left: dragGhost.x + 12,
          top: dragGhost.y - 12,
          pointerEvents: "none",
          zIndex: 9999,
        }}
        className="flex items-center gap-2 px-3 py-1.5 rounded-md text-[13px] font-medium bg-bg-sidebar border border-border-strong/60 shadow-lg text-text-base opacity-90"
      >
        {draggingFolderId ? (
          <Folder size={13} className="text-text-muted flex-shrink-0" />
        ) : (
          <FolderOpen size={13} className="text-text-muted flex-shrink-0" />
        )}
        <span>{dragGhost.name}</span>
      </div>
    )}
    </>
  );
}

// ── ProjectToolsTab ────────────────────────────────────────────────────────────

interface ProjectToolEntry {
  name: string;
  display_name: string;
  description: string;
  url: string;
  github_repo?: string;
  kind: "cli" | "doc_gen" | "analyser" | "other";
  detect_binary?: string;
  detect_dir?: string;
  plugin_id?: string;
  /** `true` = binary on PATH, `false` = not found, `null` = no detect_binary */
  detected: boolean | null;
}

function projectToolKindLabel(kind: ProjectToolEntry["kind"]): string {
  switch (kind) {
    case "cli":      return "CLI";
    case "doc_gen":  return "Doc Generator";
    case "analyser": return "Analyser";
    default:         return "Other";
  }
}

function ProjectToolAvatar({ tool, size = 28 }: { tool: ProjectToolEntry; size?: number }) {
  const [broken, setBroken] = useState(false);
  const owner = tool.github_repo ? tool.github_repo.split("/")[0] : null;
  const avatarUrl = owner ? `https://github.com/${owner}.png?size=${size * 2}` : null;
  const letter = tool.display_name.charAt(0).toUpperCase();

  if (avatarUrl && !broken) {
    return (
      <img
        src={avatarUrl}
        alt={owner ?? tool.display_name}
        width={size}
        height={size}
        className="rounded-md object-cover flex-shrink-0"
        style={{ width: size, height: size }}
        onError={() => setBroken(true)}
      />
    );
  }
  return (
    <div
      className="rounded-md flex items-center justify-center font-semibold bg-icon-skill/15 text-icon-skill flex-shrink-0"
      style={{ width: size, height: size, fontSize: Math.round(size * 0.44) }}
      aria-hidden="true"
    >
      {letter}
    </div>
  );
}

interface ProjectToolsTabProps {
  projectDir: string;
  /** Tool names explicitly added to this project (saved state). */
  projectTools: string[];
  /** Tool entries already loaded by the parent (avoids duplicate fetches). */
  entries: ProjectToolEntry[];
  loading: boolean;
  /** Called by auto-detect to request a re-fetch of entries after detection. */
  onReload: () => void;
  /** Called when the user adds or removes a tool, or auto-detects. New full list provided. */
  onToolsChange: (tools: string[]) => void;
}

function ProjectToolsTab({ projectDir, projectTools, entries, loading, onReload, onToolsChange }: ProjectToolsTabProps) {
  const [detecting, setDetecting] = useState(false);
  const [detectStatus, setDetectStatus] = useState<string | null>(null);

  /** A tool is in this project only if explicitly listed in projectTools. */
  function isInProject(name: string): boolean {
    return projectTools.includes(name);
  }

  function addTool(name: string) {
    if (isInProject(name)) return;
    onToolsChange([...projectTools, name]);
  }

  function removeTool(name: string) {
    onToolsChange(projectTools.filter((t) => t !== name));
  }

  async function handleAutoDetect() {
    if (!projectDir) {
      setDetectStatus("Set a project directory first.");
      setTimeout(() => setDetectStatus(null), 3000);
      return;
    }
    setDetecting(true);
    setDetectStatus(null);
    try {
      const detected: string[] = await invoke("autodetect_tools_for_project", { projectDir });
      if (detected.length === 0) {
        setDetectStatus("No tools detected in this project directory.");
      } else {
        // Merge detected into existing list without duplicates.
        const merged = [...new Set([...projectTools, ...detected])];
        onToolsChange(merged);
        onReload();
        const added = detected.filter((n) => !projectTools.includes(n));
        setDetectStatus(
          added.length > 0
            ? `Detected and added: ${added.join(", ")}`
            : "Already up to date — no new tools found."
        );
      }
    } catch (err) {
      console.error("Auto-detect failed:", err);
      setDetectStatus("Detection failed. Check the console for details.");
    } finally {
      setDetecting(false);
      setTimeout(() => setDetectStatus(null), 5000);
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12 text-[12px] text-text-muted">
        Loading tools…
      </div>
    );
  }

  if (entries.length === 0) {
    return (
      <section>
        <div className="flex flex-col items-center justify-center gap-4 text-center py-12 px-6">
          <Wrench size={32} className="text-text-muted opacity-40" />
          <div>
            <p className="text-[13px] font-medium text-text-base mb-1">No tools registered</p>
            <p className="text-[12px] text-text-muted leading-relaxed max-w-[320px]">
              Tools are installed by plugins. Enable a plugin in{" "}
              <span className="font-medium text-text-base">Settings → Plugins</span>{" "}
              to register tools here.
            </p>
          </div>
        </div>
      </section>
    );
  }

  const activeEntries = entries.filter((e) => isInProject(e.name));
  const otherEntries  = entries.filter((e) => !isInProject(e.name));

  return (
    <section className="space-y-4">
      {/* Header row: info + auto-detect button */}
      <div className="flex items-center gap-3">
        <div className="flex-1 flex items-start gap-2.5 px-3 py-2.5 rounded-lg bg-bg-input border border-border-strong/30 text-[12px] text-text-muted">
          <Wrench size={13} className="flex-shrink-0 mt-0.5" />
          <span>
            Tools are registered by plugins. Add them manually or use Auto-detect to scan this project.
            {!projectDir && (
              <span className="text-warning ml-1">Set a project directory to enable detection.</span>
            )}
          </span>
        </div>
        <button
          onClick={handleAutoDetect}
          disabled={detecting || !projectDir}
          className="flex items-center gap-1.5 px-3 py-2 text-[12px] font-medium border border-border-strong/50 rounded-lg text-text-muted hover:text-text-base hover:bg-bg-input transition-colors disabled:opacity-40 flex-shrink-0"
          title={!projectDir ? "Set a project directory first" : "Scan the project directory for installed tools"}
        >
          <RefreshCw size={12} className={detecting ? "animate-spin" : ""} />
          {detecting ? "Detecting…" : "Auto-detect"}
        </button>
      </div>

      {/* Status message */}
      {detectStatus && (
        <p className="text-[12px] text-text-muted px-1">{detectStatus}</p>
      )}

      {/* Active in this project */}
      {activeEntries.length > 0 && (
        <div>
          <div className="flex items-center gap-2 mb-2">
            <CheckCircle2 size={12} className="text-green-400" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
              In this project
            </span>
          </div>
          <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
            {activeEntries.map((entry) => (
              <ProjectToolRow
                key={entry.name}
                entry={entry}
                active
                onRemove={() => removeTool(entry.name)}
              />
            ))}
          </div>
        </div>
      )}

      {/* Available tools not in this project */}
      {otherEntries.length > 0 && (
        <div>
          <div className="flex items-center gap-2 mb-2">
            <MinusCircle size={12} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
              Available tools
            </span>
          </div>
          <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
            {otherEntries.map((entry) => (
              <ProjectToolRow
                key={entry.name}
                entry={entry}
                active={false}
                onAdd={() => addTool(entry.name)}
              />
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

// ── ToolInfoSidebar ───────────────────────────────────────────────────────────
// Compact right-hand sidebar shown on every tool detail page.

interface ToolInfoSidebarProps {
  entry: ProjectToolEntry;
  active: boolean;
  onAdd: () => void;
  onRemove: () => void;
}

function ToolInfoSidebar({ entry, active, onAdd, onRemove }: ToolInfoSidebarProps) {
  return (
    <aside className="w-56 flex-shrink-0 flex flex-col gap-4">
      {/* Avatar + name */}
      <div className="flex flex-col items-center gap-2 text-center">
        <ProjectToolAvatar tool={entry} size={44} />
        <div>
          <p className="text-[13px] font-semibold text-text-base leading-tight">{entry.display_name}</p>
          <code className="text-[11px] font-mono text-text-muted">{entry.name}</code>
        </div>
        <span className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded bg-bg-input border border-border-strong/40 text-text-muted">
          <Terminal size={9} />
          {projectToolKindLabel(entry.kind)}
        </span>
      </div>

      {/* Status + action */}
      <div className="flex flex-col gap-2">
        {active ? (
          <>
            <span className="flex items-center justify-center gap-1 text-[11px] text-green-400">
              <CheckCircle2 size={12} /> Active in project
            </span>
            <button
              onClick={onRemove}
              className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 text-[12px] font-medium rounded-lg border border-danger/30 text-danger hover:bg-danger/10 transition-colors"
            >
              <X size={12} /> Remove
            </button>
          </>
        ) : (
          <>
            <span className="flex items-center justify-center gap-1 text-[11px] text-text-muted">
              <MinusCircle size={12} /> Not in project
            </span>
            <button
              onClick={onAdd}
              className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 text-[12px] font-medium rounded-lg border border-brand/40 text-brand hover:bg-brand/10 transition-colors"
            >
              <Plus size={12} /> Add to project
            </button>
          </>
        )}
      </div>

      {/* Metadata rows */}
      <div className="flex flex-col gap-1.5 text-[11px]">
        {entry.description && (
          <p className="text-text-muted leading-relaxed">{entry.description}</p>
        )}
        {entry.url && (
          <a
            href={entry.url}
            target="_blank"
            rel="noreferrer"
            onClick={handleExternalLinkClick(entry.url)}
            className="flex items-center gap-1 text-brand hover:underline truncate"
          >
            <Globe size={10} className="flex-shrink-0" />
            {entry.url}
            <ExternalLink size={10} className="flex-shrink-0" />
          </a>
        )}
        {entry.github_repo && (
          <a
            href={`https://github.com/${entry.github_repo}`}
            target="_blank"
            rel="noreferrer"
            onClick={handleExternalLinkClick(`https://github.com/${entry.github_repo}`)}
            className="flex items-center gap-1 text-brand hover:underline truncate"
          >
            <Globe size={10} className="flex-shrink-0" />
            {entry.github_repo}
            <ExternalLink size={10} className="flex-shrink-0" />
          </a>
        )}
        {entry.detect_dir && (
          <div className="flex items-center gap-1 text-text-muted font-mono">
            <span className="text-[9px] px-1 py-0.5 rounded bg-bg-input border border-border-strong/40 uppercase tracking-wider non-mono text-[9px]">dir</span>
            {entry.detect_dir}/
          </div>
        )}
        {entry.detect_binary && (
          <div className="flex items-center gap-1 text-text-muted font-mono">
            <span className="text-[9px] px-1 py-0.5 rounded bg-bg-input border border-border-strong/40 uppercase tracking-wider non-mono text-[9px]">bin</span>
            {entry.detect_binary}
            {entry.detected === true && <span className="non-mono text-green-400 text-[10px]">✓</span>}
            {entry.detected === false && <span className="non-mono text-text-muted text-[10px]">✗</span>}
          </div>
        )}
        {entry.plugin_id && (
          <div className="flex items-center gap-1 text-text-muted">
            <Puzzle size={10} className="flex-shrink-0" />
            <span className="font-mono">{entry.plugin_id}</span>
          </div>
        )}
      </div>
    </aside>
  );
}

// ── ProjectToolRow ─────────────────────────────────────────────────────────────

interface ProjectToolRowProps {
  entry: ProjectToolEntry;
  active: boolean;
  onAdd?: () => void;
  onRemove?: () => void;
}

function ProjectToolRow({ entry, active, onAdd, onRemove }: ProjectToolRowProps) {
  return (
    <div className="flex items-center gap-3 px-4 py-3 hover:bg-surface-hover transition-colors group">
      <ProjectToolAvatar tool={entry} size={28} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-[13px] font-medium text-text-base truncate">
            {entry.display_name}
          </span>
          <span className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar border border-border-strong/40 text-text-muted">
            <Terminal size={9} />
            {projectToolKindLabel(entry.kind)}
          </span>
        </div>
        {entry.description && (
          <p className="text-[11px] text-text-muted mt-0.5 truncate">{entry.description}</p>
        )}
        <div className="flex items-center gap-3 mt-1 flex-wrap">
          {entry.detect_dir && (
            <span className="flex items-center gap-1 text-[11px] text-text-muted font-mono">
              <Globe size={10} />
              {entry.detect_dir}/
            </span>
          )}
          {entry.detect_binary && (
            <span className="flex items-center gap-1 text-[11px] text-text-muted font-mono">
              <Terminal size={10} />
              {entry.detect_binary}
            </span>
          )}
          {entry.plugin_id && (
            <span className="flex items-center gap-1 text-[11px] text-text-muted">
              <Puzzle size={10} />
              {entry.plugin_id}
            </span>
          )}
          {entry.url && (
            <a
              href={entry.url}
              target="_blank"
              rel="noreferrer"
              onClick={handleExternalLinkClick(entry.url)}
              className="flex items-center gap-1 text-[11px] text-brand hover:underline"
            >
              <ExternalLink size={10} />
              Docs
            </a>
          )}
        </div>
      </div>
      <div className="flex-shrink-0 flex items-center gap-2">
        {active ? (
          <>
            <span className="flex items-center gap-1 text-[11px] text-green-400">
              <CheckCircle2 size={11} />
              Active
            </span>
            <button
              onClick={onRemove}
              className="opacity-0 group-hover:opacity-100 p-1 rounded text-text-muted hover:text-danger hover:bg-danger/10 transition-all"
              title="Remove from project"
            >
              <X size={12} />
            </button>
          </>
        ) : (
          <button
            onClick={onAdd}
            className="flex items-center gap-1 text-[11px] font-medium px-2 py-1 rounded border border-border-strong/50 text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors"
            title="Add to project"
          >
            <Plus size={11} />
            Add
          </button>
        )}
      </div>
    </div>
  );
}

// ── ProjectToolDetailPanel ────────────────────────────────────────────────────

interface ProjectToolDetailPanelProps {
  entry: ProjectToolEntry;
  projectDir: string;
  active: boolean;
  onAdd: () => void;
  onRemove: () => void;
}

function ProjectToolDetailPanel({ entry, projectDir, active, onAdd, onRemove }: ProjectToolDetailPanelProps) {
  const CustomPanel = getToolPanel(entry.name);
  
  if (CustomPanel) {
    return (
      <CustomPanel
        projectDir={projectDir}
        sidebar={<ToolInfoSidebar entry={entry} active={active} onAdd={onAdd} onRemove={onRemove} />}
      />
    );
  }

  return (
    <div className="flex gap-6 items-start">
      <section className="flex-1 min-w-0">
        <div className="flex flex-col items-center justify-center gap-3 py-16 text-center text-text-muted">
          <ProjectToolAvatar tool={entry} size={40} />
          <p className="text-[12px]">No additional detail view for this tool.</p>
        </div>
      </section>
      <ToolInfoSidebar entry={entry} active={active} onAdd={onAdd} onRemove={onRemove} />
    </div>
  );
}

// ── End of File ────────────────────────────────────────────────────────────────
