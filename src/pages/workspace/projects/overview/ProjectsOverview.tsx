// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AgentIcon } from "../../../../components/AgentIcon";
import {
  AlertCircle, Bot, Check, CheckCircle2, ChevronDown, Code, FolderOpen,
  LayoutGrid, Plus, RefreshCw, ScrollText, Search, Server, Table2, Terminal,
} from "lucide-react";
import { relativeTime } from "../helpers";
import type { Project } from "../types";

interface ProjectsOverviewProps {
  projects: string[];
  projectsLoading: boolean;
  projectDetails: Map<string, Project>;
  driftByProject: Record<string, boolean>;
  onSelect: (name: string) => void;
  onCreate: () => void;
  onSyncAll?: () => void;
  syncAllStatus?: "idle" | "syncing";
  /** When set, only show projects belonging to this group. */
  filterGroup?: string | null;
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
  const isMissingDir = project?.directory_missing === true;
  const isConfigured = !!(project?.directory && (project?.agents?.length ?? 0) > 0);

  const borderClass = isMissingDir
    ? "border-danger/30 hover:border-danger/50"
    : isDrifted
    ? "border-warning/30 hover:border-warning/50"
    : "border-border-strong/40 hover:border-border-strong/70";

  const totalSkills = (project?.skills?.length ?? 0) + (project?.custom_skills?.length ?? 0);
  const mcpCount = project?.mcp_servers?.length ?? 0;
  const totalRules = Object.values(project?.file_rules ?? {}).reduce((sum, arr) => sum + arr.length, 0) + (project?.custom_rules?.length ?? 0);
  const subAgentCount = (project?.custom_agents?.length ?? 0) + (project?.user_agents?.length ?? 0);
  const commandCount = (project?.custom_commands?.length ?? 0) + (project?.user_commands?.length ?? 0);

  return (
    <button
      onClick={() => onSelect(name)}
      className={`group relative w-full h-full text-left bg-bg-input border ${borderClass} rounded-xl p-5 flex flex-col gap-3 transition-all hover:bg-surface-hover hover:-translate-y-0.5`}
    >
      {/* Sync status — top right */}
      {isConfigured && (
        <div className="absolute top-4 right-4">
          <ProjectStatusBadge drift={drift} />
        </div>
      )}

      {/* Row 1: icon + title + directory */}
      <div className="flex items-start gap-3 pr-20">
        <div
          className={`flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-lg border ${
            isMissingDir
              ? "border-danger/30 bg-danger/10"
              : isDrifted
              ? "border-warning/30 bg-warning/10"
              : "border-brand/20 bg-brand/10"
          }`}
        >
          {isMissingDir ? (
            <AlertCircle size={16} className="flex-shrink-0 text-danger" />
          ) : (
            <FolderOpen
              size={16}
              className={`flex-shrink-0 ${isDrifted ? "text-warning" : "text-brand"}`}
            />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-semibold text-text-base leading-snug truncate">{name}</div>
        </div>
      </div>

      {/* Row 2: agent chips */}
      {(project?.agents?.length ?? 0) > 0 ? (
        <div className="flex items-center gap-1.5 flex-wrap">
          {(project?.agents ?? []).map((agentId) => (
            <span
              key={agentId}
              className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted"
            >
              <AgentIcon agentId={agentId} size={9} />
              {agentId}
            </span>
          ))}
        </div>
      ) : (
        <div className="flex items-center gap-1.5 text-[11px] text-warning/70">
          <AlertCircle size={10} className="flex-shrink-0" />
          <span>No agents configured</span>
        </div>
      )}

      {/* Missing directory warning */}
      {isMissingDir && (
        <div className="flex items-center gap-1.5 text-[11px] text-danger/80">
          <AlertCircle size={10} className="flex-shrink-0" />
          <span>Folder not found — relink required</span>
        </div>
      )}

      {/* Row 3: stats footer */}
      <div className="mt-auto flex w-full items-center gap-3 pt-3 border-t border-border-strong/30 text-[11px] text-text-muted">
        <span className="flex items-center gap-1">
          <Code size={10} />
          {totalSkills}
        </span>
        <span className="flex items-center gap-1">
          <Server size={10} />
          {mcpCount}
        </span>
        <span className="flex items-center gap-1">
          <ScrollText size={10} />
          {totalRules}
        </span>
        <span className="flex items-center gap-1">
          <Bot size={10} />
          {subAgentCount}
        </span>
        <span className="flex items-center gap-1">
          <Terminal size={10} />
          {commandCount}
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

  // Unique agent ids, skill names, and MCP server names across all projects
  const agentSet = new Set<string>();
  const skillSet = new Set<string>();
  const mcpSet = new Set<string>();
  let fullyConfigured = 0;
  for (const name of projects) {
    const p = projectDetails.get(name);
    if (!p) continue;
    (p.agents ?? []).forEach((a) => agentSet.add(a));
    (p.skills ?? []).forEach((s) => skillSet.add(s));
    (p.custom_skills ?? []).forEach((s) => skillSet.add(s.name));
    (p.mcp_servers ?? []).forEach((m) => mcpSet.add(m));
    if ((p.agents?.length ?? 0) > 0 && !!p.directory) fullyConfigured++;
  }
  const totalSkills = skillSet.size;
  const totalMcp = mcpSet.size;

  // Show a compact progress-like bar for synced/drifted/checking ratio
  const syncedPct = total > 0 ? Math.round((synced / total) * 100) : 0;
  const driftedPct = total > 0 ? Math.round((drifted / total) * 100) : 0;
  const checkingPct = total > 0 ? Math.max(0, 100 - syncedPct - driftedPct) : 0;

  return (
    <div className="border-b border-border-strong/40 bg-bg-input overflow-hidden">
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

export function ProjectsOverview({ projects, projectsLoading, projectDetails, driftByProject, onSelect, onCreate, onSyncAll, syncAllStatus, filterGroup = null }: ProjectsOverviewProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [sortOrder, setSortOrder] = useState<"alphabetical" | "created" | "updated" | "last_activity">("alphabetical");
  // Projects can be displayed as a card grid or a compact table. The choice is
  // persisted per-machine so it survives navigation and restarts.
  const VIEW_MODE_KEY = "automatic.projects.viewMode";
  const [viewMode, setViewMode] = useState<"grid" | "table">(() => {
    const stored = localStorage.getItem(VIEW_MODE_KEY);
    return stored === "table" ? "table" : "grid";
  });
  const selectViewMode = (mode: "grid" | "table") => {
    setViewMode(mode);
    localStorage.setItem(VIEW_MODE_KEY, mode);
  };
  const [groupProjectNames, setGroupProjectNames] = useState<Set<string> | null>(null);
  /** For "__ungrouped__" filter: set of all projects that ARE in some group. */
  const [allGroupedNames, setAllGroupedNames] = useState<Set<string> | null>(null);

  /** Load all grouped project names (union of every group's members). */
  const loadAllGroupedNames = async (): Promise<Set<string>> => {
    const grouped = new Set<string>();
    try {
      const groupNames: string[] = await invoke("list_groups");
      for (const name of groupNames) {
        try {
          const raw: string = await invoke("read_group", { name });
          const g = JSON.parse(raw);
          for (const p of (g.projects ?? [])) grouped.add(p);
        } catch { /* skip */ }
      }
    } catch { /* skip */ }
    return grouped;
  };

  // Load group members when filterGroup changes
  useEffect(() => {
    if (!filterGroup) { setGroupProjectNames(null); setAllGroupedNames(null); return; }
    let cancelled = false;
    if (filterGroup === "__ungrouped__") {
      loadAllGroupedNames().then((grouped) => {
        if (!cancelled) setAllGroupedNames(grouped);
      });
    } else {
      (async () => {
        try {
          const raw: string = await invoke("read_group", { name: filterGroup });
          const g = JSON.parse(raw);
          if (!cancelled) setGroupProjectNames(new Set(g.projects ?? []));
        } catch {
          if (!cancelled) setGroupProjectNames(new Set());
        }
      })();
    }
    return () => { cancelled = true; };
  }, [filterGroup]);

  // Re-load group members when groups change externally
  useEffect(() => {
    if (!filterGroup) return;
    const handler = () => {
      if (filterGroup === "__ungrouped__") {
        loadAllGroupedNames().then(setAllGroupedNames);
      } else {
        invoke<string>("read_group", { name: filterGroup }).then((raw) => {
          const g = JSON.parse(raw);
          setGroupProjectNames(new Set(g.projects ?? []));
        }).catch(() => setGroupProjectNames(new Set()));
      }
    };
    window.addEventListener("groups-updated", handler);
    return () => window.removeEventListener("groups-updated", handler);
  }, [filterGroup]);

  const getSortTimestamp = (project: Project | undefined, key: "created" | "updated" | "last_activity"): number => {
    if (!project) return 0;
    if (key === "created") return new Date(project.created_at ?? 0).getTime();
    if (key === "updated") return new Date(project.updated_at ?? 0).getTime();
    return new Date(project.last_activity ?? project.updated_at ?? project.created_at ?? 0).getTime();
  };

  const sortNames = (names: string[]) => {
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

  // Apply group filter then search filter
  const baseProjects = filterGroup === "__ungrouped__" && allGroupedNames
    ? projects.filter((n) => !allGroupedNames.has(n))
    : groupProjectNames
      ? projects.filter((n) => groupProjectNames.has(n))
      : projects;
  const filteredProjects = sortNames(baseProjects.filter(matchesSearch));

  const renderCardGrid = (names: string[]) => (
    <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
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

  const renderTable = (names: string[]) => (
    <div className="overflow-x-auto rounded-lg border border-border-strong/40 bg-bg-input/40">
      <table className="w-full border-collapse text-[12px]">
        <thead>
          <tr className="border-b border-border-strong/40 text-left text-[11px] font-medium uppercase tracking-wide text-text-muted">
            <th className="px-3 py-2 font-medium">Project</th>
            <th className="px-3 py-2 font-medium">Status</th>
            <th className="px-3 py-2 font-medium text-right" title="Agents">Agents</th>
            <th className="px-3 py-2 font-medium text-right" title="Skills">Skills</th>
            <th className="px-3 py-2 font-medium text-right" title="MCP servers">MCP</th>
            <th className="px-3 py-2 font-medium text-right" title="Rules">Rules</th>
            <th className="px-3 py-2 font-medium text-right" title="Sub-agents">Sub-agents</th>
            <th className="px-3 py-2 font-medium text-right" title="Commands">Commands</th>
            <th className="px-3 py-2 font-medium text-right">Last activity</th>
          </tr>
        </thead>
        <tbody>
          {names.map((name) => {
            const project = projectDetails.get(name);
            const drift = driftByProject[name];
            // Mirror the per-card derivations so both views report identical counts
            // and an identical sync state. A project is only "synced/drifted" once
            // it is configured (has a directory and at least one agent); otherwise
            // the card suppresses the badge and surfaces the configuration gap, and
            // the table must do the same to stay consistent.
            const isMissingDir = project?.directory_missing === true;
            const isConfigured = !!(project?.directory && (project?.agents?.length ?? 0) > 0);
            const agentCount = project?.agents?.length ?? 0;
            const skillsCount = (project?.skills?.length ?? 0) + (project?.custom_skills?.length ?? 0);
            const mcpCount = project?.mcp_servers?.length ?? 0;
            const rulesCount =
              Object.values(project?.file_rules ?? {}).reduce((sum, arr) => sum + arr.length, 0) +
              (project?.custom_rules?.length ?? 0);
            const subAgentCount = (project?.custom_agents?.length ?? 0) + (project?.user_agents?.length ?? 0);
            const commandsCount = (project?.custom_commands?.length ?? 0) + (project?.user_commands?.length ?? 0);
            const directory = project?.directory ?? "";
            const lastActivity = project?.last_activity ?? project?.updated_at ?? project?.created_at ?? null;
            return (
              <tr
                key={name}
                onClick={() => onSelect(name)}
                className="group cursor-pointer border-b border-border-strong/20 last:border-b-0 transition-colors hover:bg-bg-input/70"
              >
                <td className="px-3 py-2">
                  <div className="flex items-center gap-2.5 min-w-0">
                    <div className="w-7 h-7 rounded-md bg-gradient-to-br from-brand/20 to-brand/5 border border-brand/20 flex items-center justify-center shrink-0">
                      <FolderOpen size={13} className="text-brand" />
                    </div>
                    <div className="min-w-0">
                      <div className="text-[13px] font-medium text-text-base truncate group-hover:text-brand transition-colors">
                        {name}
                      </div>
                      {directory && (
                        <div className="text-[11px] text-text-muted truncate">{directory}</div>
                      )}
                    </div>
                  </div>
                </td>
                <td className="px-3 py-2">
                  {isMissingDir ? (
                    <span className="inline-flex items-center gap-1 text-[10px] font-medium text-danger bg-danger/10 border border-danger/30 rounded-full px-2 py-0.5">
                      <AlertCircle size={9} /> Folder missing
                    </span>
                  ) : !isConfigured ? (
                    <span className="inline-flex items-center gap-1 text-[10px] font-medium text-warning bg-warning/10 border border-warning/30 rounded-full px-2 py-0.5">
                      <AlertCircle size={9} /> No agents
                    </span>
                  ) : drift === true ? (
                    <span className="inline-flex items-center gap-1 text-[10px] font-medium text-warning bg-warning/10 border border-warning/30 rounded-full px-2 py-0.5">
                      <AlertCircle size={9} /> Drifted
                    </span>
                  ) : drift === false ? (
                    <span className="inline-flex items-center gap-1 text-[10px] font-medium text-success bg-success/10 border border-success/30 rounded-full px-2 py-0.5">
                      <CheckCircle2 size={9} /> Synced
                    </span>
                  ) : (
                    <span className="text-[11px] text-text-muted">—</span>
                  )}
                </td>
                <td className="px-3 py-2 text-right tabular-nums text-text-base">{agentCount}</td>
                <td className="px-3 py-2 text-right tabular-nums text-text-base">{skillsCount}</td>
                <td className="px-3 py-2 text-right tabular-nums text-text-base">{mcpCount}</td>
                <td className="px-3 py-2 text-right tabular-nums text-text-base">{rulesCount}</td>
                <td className="px-3 py-2 text-right tabular-nums text-text-base">{subAgentCount}</td>
                <td className="px-3 py-2 text-right tabular-nums text-text-base">{commandsCount}</td>
                <td className="px-3 py-2 text-right text-[11px] text-text-muted whitespace-nowrap">
                  {lastActivity ? relativeTime(lastActivity) : "—"}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );

  return (
    <div className="flex-1 h-full overflow-y-auto custom-scrollbar bg-bg-base">
      {/* Top bar */}
      <div className="px-6 py-3 border-b border-border-strong/40 flex items-center justify-between bg-bg-base/50 flex-shrink-0">
        <span className="text-[13px] font-semibold text-text-muted tracking-wide uppercase">
          {filterGroup === "__ungrouped__" ? "Other Projects" : filterGroup ? filterGroup : "Projects"}
        </span>
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
          <div className="flex items-center rounded-md border border-border-strong/50 bg-bg-input p-0.5" role="group" aria-label="Project view mode">
            <button
              onClick={() => selectViewMode("grid")}
              aria-label="Card view"
              aria-pressed={viewMode === "grid"}
              title="Card view"
              className={`flex h-6 w-6 items-center justify-center rounded transition-colors ${
                viewMode === "grid"
                  ? "bg-brand/15 text-brand"
                  : "text-text-muted hover:text-text-base"
              }`}
            >
              <LayoutGrid size={13} />
            </button>
            <button
              onClick={() => selectViewMode("table")}
              aria-label="Table view"
              aria-pressed={viewMode === "table"}
              title="Table view"
              className={`flex h-6 w-6 items-center justify-center rounded transition-colors ${
                viewMode === "table"
                  ? "bg-brand/15 text-brand"
                  : "text-text-muted hover:text-text-base"
              }`}
            >
              <Table2 size={13} />
            </button>
          </div>
          <div className="relative">
            <select
              value={sortOrder}
              onChange={(e) => setSortOrder(e.target.value as "alphabetical" | "created" | "updated" | "last_activity")}
              className="h-7 min-w-[120px] appearance-none rounded-md border border-border-strong/50 bg-bg-input px-2.5 pr-7 text-[12px] text-text-base shadow-none focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60"
              aria-label="Sort projects"
            >
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
          {onSyncAll && projects.some((n) => driftByProject[n] === true) && (
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
            <Plus size={12} /> Add Project
          </button>
        </div>
      </div>

      {/* Health overview bar — flush full-width */}
      {!projectsLoading && baseProjects.length > 0 && (
        <ProjectsHealthBar
          projects={baseProjects}
          projectDetails={projectDetails}
          driftByProject={driftByProject}
        />
      )}

      <div className="p-6 space-y-5">
        {/* Empty state */}
        {!projectsLoading && baseProjects.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <div className="w-16 h-16 rounded-2xl border border-brand/20 bg-brand/10 flex items-center justify-center mb-5">
              <FolderOpen size={24} className="text-brand" />
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
        ) : filteredProjects.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center border border-border-strong/30 rounded-lg bg-bg-input/40">
            <p className="text-[13px] text-text-base mb-1">No matching projects</p>
            <p className="text-[12px] text-text-muted">Try another search term.</p>
          </div>
        ) : viewMode === "table" ? (
          renderTable(filteredProjects)
        ) : (
          renderCardGrid(filteredProjects)
        )}
      </div>
    </div>
  );
}
