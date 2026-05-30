// Pure helper functions for the Projects workspace feature.
//
// Extracted verbatim from Projects.tsx as part of a behavior-preserving refactor.
// This file is `.tsx` because `activityMeta` returns JSX.

import {
  Check,
  Code,
  Server,
  Bot,
  FolderOpen,
  RefreshCw,
  History,
} from "lucide-react";
import type { Project, ProjectToolEntry } from "./types";

export function parseInvokeResult<T>(value: unknown): T {
  if (typeof value === "string") {
    return JSON.parse(value) as T;
  }

  return value as T;
}

/** Map a Settings::active_agent ID to a human-readable display label. */
export function agentIdToLabel(id: string): string {
  const labels: Record<string, string> = {
    anthropic: "Claude",
    openai: "OpenAI",
    "github-models": "GitHub Models",
    "workers-ai": "Workers AI",
    zai: "Z.ai",
    "opencode-zen": "OpenCode Zen",
  };
  return labels[id] ?? id;
}

/** Returns a relative time string ("just now", "5 min ago", "2 days ago", etc.) */
export function relativeTime(iso: string): string {
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
export function activityMeta(event: string): { icon: React.ReactNode; dot: string } {
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

export function emptyProject(name: string): Project {
  return {
    name,
    description: "",
    directory: "",
    skills: [],
    mcp_servers: [],
    disabled_mcp_servers: [],
    providers: [],
    agents: [],
    user_agents: [],
    user_commands: [],
    hooks: [],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    file_rules: {},
    instruction_mode: "per-agent",
    custom_commands: [],
  };
}

export function isHttpDocPath(path: string): boolean {
  return path.startsWith("http://") || path.startsWith("https://");
}

export function isManagedDocNotePath(path: string): boolean {
  return path.startsWith(".automatic/docs/");
}

export function getProjectRelativeDocPath(projectDirectory: string | undefined, path: string): string | null {
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

export function projectToolKindLabel(kind: ProjectToolEntry["kind"]): string {
  switch (kind) {
    case "cli":      return "CLI";
    case "doc_gen":  return "Doc Generator";
    case "analyser": return "Analyser";
    case "planning": return "Planning";
    default:         return "Other";
  }
}
