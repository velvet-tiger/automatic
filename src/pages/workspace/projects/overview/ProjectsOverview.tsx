// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AgentIcon } from "../../../../components/AgentIcon";
import {
  AlertCircle, ChevronDown, FolderOpen, Layers, LayoutGrid, Plus, RefreshCw, Search, Table2,
} from "lucide-react";
import { relativeTime } from "../helpers";
import type { Project } from "../types";

/** One section in the grouped projects view (a named group, or ungrouped). */
interface ProjectGroupSection {
  id: string;
  label: string;
  projects: string[];
}

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

function ProjectStatusBadge({ drift, missingDir }: { drift: boolean | undefined; missingDir?: boolean }) {
  if (missingDir) {
    return <span className="text-[11px] text-danger">Missing folder</span>;
  }
  if (drift === true) {
    return <span className="text-[11px] text-warning">Drifted</span>;
  }
  if (drift === false) {
    return <span className="text-[11px] text-text-muted/45">Synced</span>;
  }
  return <span className="text-[11px] text-text-muted/30">Checking…</span>;
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
  const isMissingDir = project?.directory_missing === true;
  const isConfigured = !!(project?.directory && (project?.agents?.length ?? 0) > 0);

  const totalSkills = (project?.skills?.length ?? 0) + (project?.custom_skills?.length ?? 0);
  const mcpCount = project?.mcp_servers?.length ?? 0;
  const totalRules =
    Object.values(project?.file_rules ?? {}).reduce((sum, arr) => sum + arr.length, 0) +
    (project?.custom_rules?.length ?? 0);
  const subAgentCount = (project?.custom_agents?.length ?? 0) + (project?.user_agents?.length ?? 0);
  const commandCount = (project?.custom_commands?.length ?? 0) + (project?.user_commands?.length ?? 0);
  const dateLabel = project?.updated_at
    ? new Date(project.updated_at).toLocaleDateString(undefined, { month: "short", day: "numeric" })
    : null;

  const metricParts = [
    `${totalSkills} skills`,
    `${mcpCount} mcp`,
    `${totalRules} rules`,
    `${subAgentCount} agents`,
    `${commandCount} cmds`,
  ];
  if (dateLabel) metricParts.push(dateLabel);

  return (
    <button
      onClick={() => onSelect(name)}
      className="group w-full text-left bg-bg-input border border-border-strong/35 hover:border-border-strong/60 rounded-lg px-3 py-2.5 flex flex-col gap-1 transition-colors hover:bg-surface-hover"
    >
      {/*
        Buttons shrink-wrap flex children unless the row is explicitly full-width.
        Use justify-between + w-full so agents pin to the right edge.
      */}
      <div className="flex w-full items-start justify-between gap-2.5">
        <div className="flex min-w-0 items-start gap-2.5">
          <div className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md border border-border-strong/40 text-text-muted/70">
            {isMissingDir ? (
              <AlertCircle size={13} className="flex-shrink-0 text-danger" />
            ) : (
              <FolderOpen size={13} className="flex-shrink-0" />
            )}
          </div>
          <div className="min-w-0">
            <div className="text-[13px] font-medium text-text-base leading-snug truncate">{name}</div>
            {isConfigured ? (
              <ProjectStatusBadge drift={drift} missingDir={isMissingDir} />
            ) : (
              <div className="text-[11px] text-warning/80">No agents configured</div>
            )}
          </div>
        </div>
        {(project?.agents?.length ?? 0) > 0 && (
          <div className="flex flex-shrink-0 items-center gap-1.5 pt-0.5">
            {(project?.agents ?? []).map((agentId) => (
              <AgentIcon key={agentId} agentId={agentId} size={14} />
            ))}
          </div>
        )}
      </div>

      {isMissingDir && (
        <div className="pl-8 text-[11px] text-danger/80">Folder not found — relink required</div>
      )}

      <div className="pl-8 grid grid-rows-[0fr] group-hover:grid-rows-[1fr] group-focus-visible:grid-rows-[1fr] transition-[grid-template-rows] duration-200 ease-out">
        <div className="overflow-hidden text-[11px] text-text-muted/40">
          {metricParts.join(" · ")}
        </div>
      </div>
    </button>
  );
}

// ── Projects Health Bar ───────────────────────────────────────────────────────

interface ProjectsHealthBarProps {
  projects: string[];
  projectDetails: Map<string, Project>;
  driftByProject: Record<string, boolean>;
  /** Optional label shown on the left (group name or "Projects"). */
  label?: string;
}

function ProjectsHealthBar({ projects, projectDetails, driftByProject, label }: ProjectsHealthBarProps) {
  if (projects.length === 0) return null;

  const total = projects.length;
  const synced = projects.filter((n) => driftByProject[n] === false).length;
  const drifted = projects.filter((n) => driftByProject[n] === true).length;
  const checking = projects.filter((n) => driftByProject[n] === undefined).length;

  let fullyConfigured = 0;
  for (const name of projects) {
    const p = projectDetails.get(name);
    if (!p) continue;
    if ((p.agents?.length ?? 0) > 0 && !!p.directory) fullyConfigured++;
  }

  const syncedPct = total > 0 ? Math.round((synced / total) * 100) : 0;
  const driftedPct = total > 0 ? Math.round((drifted / total) * 100) : 0;
  const checkingPct = total > 0 ? Math.max(0, 100 - syncedPct - driftedPct) : 0;
  const title = label && label.length > 0 ? label : "Projects";

  return (
    <div className="border-b border-border-strong/30 px-5 py-2.5 flex items-center gap-4 bg-bg-base">
      <div className="flex items-baseline gap-2 min-w-0 flex-shrink-0">
        <span className="text-[13px] font-semibold text-text-base truncate">{title}</span>
        <span className="text-[12px] text-text-muted/50 tabular-nums whitespace-nowrap">
          {total} {total === 1 ? "project" : "projects"}
        </span>
      </div>

      {checking < total && (
        <div
          className="w-40 max-w-[30%] h-1 rounded-full overflow-hidden flex flex-shrink-0"
          style={{ background: "var(--health-checking)" }}
          title={`${synced} synced, ${drifted} drifted, ${checking} checking`}
        >
          {syncedPct > 0 && (
            <div className="h-full transition-all" style={{ width: `${syncedPct}%`, background: "var(--health-synced)" }} />
          )}
          {driftedPct > 0 && (
            <div className="h-full transition-all" style={{ width: `${driftedPct}%`, background: "var(--health-drifted)" }} />
          )}
          {checkingPct > 0 && (
            <div className="h-full transition-all" style={{ width: `${checkingPct}%`, background: "var(--health-checking)" }} />
          )}
        </div>
      )}

      <div className="flex items-center gap-2.5 text-[11px] text-text-muted/60 min-w-0 flex-wrap">
        {synced > 0 && <span>{synced} synced</span>}
        {drifted > 0 && (
          <span style={{ color: "var(--health-drifted)" }}>{drifted} drifted</span>
        )}
        {checking > 0 && <span>{checking} checking…</span>}
        {fullyConfigured < total && (
          <span>{total - fullyConfigured} unconfigured</span>
        )}
      </div>
    </div>
  );
}

/** Compact section heading used when projects are partitioned by group. */
function GroupSectionHeader({
  label,
  projects,
  driftByProject,
}: {
  label: string;
  projects: string[];
  driftByProject: Record<string, boolean>;
}) {
  const total = projects.length;
  const synced = projects.filter((n) => driftByProject[n] === false).length;
  const drifted = projects.filter((n) => driftByProject[n] === true).length;

  return (
    <div className="flex items-baseline gap-2.5 min-w-0">
      <h2 className="text-[13px] font-semibold text-text-base truncate">{label}</h2>
      <span className="text-[12px] text-text-muted/50 tabular-nums whitespace-nowrap">
        {total} {total === 1 ? "project" : "projects"}
      </span>
      {(synced > 0 || drifted > 0) && (
        <span className="text-[11px] text-text-muted/50 whitespace-nowrap">
          {synced > 0 && `${synced} synced`}
          {synced > 0 && drifted > 0 && " · "}
          {drifted > 0 && (
            <span style={{ color: "var(--health-drifted)" }}>{drifted} drifted</span>
          )}
        </span>
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
  const SHOW_GROUPS_KEY = "automatic.projects.showGroups";
  const [viewMode, setViewMode] = useState<"grid" | "table">(() => {
    const stored = localStorage.getItem(VIEW_MODE_KEY);
    return stored === "table" ? "table" : "grid";
  });
  const selectViewMode = (mode: "grid" | "table") => {
    setViewMode(mode);
    localStorage.setItem(VIEW_MODE_KEY, mode);
  };
  // When true (and not filtered to a single group), render all projects in
  // sections by Project Group, with ungrouped projects last.
  const [showGroups, setShowGroups] = useState(() => {
    return localStorage.getItem(SHOW_GROUPS_KEY) === "true";
  });
  const toggleShowGroups = () => {
    setShowGroups((prev) => {
      const next = !prev;
      localStorage.setItem(SHOW_GROUPS_KEY, String(next));
      return next;
    });
  };
  const [groupProjectNames, setGroupProjectNames] = useState<Set<string> | null>(null);
  /** For "__ungrouped__" filter: set of all projects that ARE in some group. */
  const [allGroupedNames, setAllGroupedNames] = useState<Set<string> | null>(null);
  /** Sections for the "Show Groups" layout (all groups + ungrouped). null = not loaded yet. */
  const [groupSections, setGroupSections] = useState<ProjectGroupSection[] | null>(null);

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

  /** Build ordered sections: each group (A–Z), then ungrouped leftovers. */
  const loadGroupSections = useCallback(async (allProjects: string[]): Promise<ProjectGroupSection[]> => {
    const projectSet = new Set(allProjects);
    const sections: ProjectGroupSection[] = [];
    const inAnyGroup = new Set<string>();
    try {
      const groupNames: string[] = await invoke("list_groups");
      for (const name of [...groupNames].sort((a, b) => a.localeCompare(b))) {
        try {
          const raw: string = await invoke("read_group", { name });
          const g = JSON.parse(raw);
          const members = (g.projects ?? []).filter((p: string) => projectSet.has(p));
          for (const p of members) inAnyGroup.add(p);
          if (members.length === 0) continue;
          sections.push({ id: name, label: name, projects: members });
        } catch { /* skip unreadable group */ }
      }
    } catch { /* no groups */ }
    const ungrouped = allProjects.filter((p) => !inAnyGroup.has(p));
    if (ungrouped.length > 0) {
      sections.push({ id: "__ungrouped__", label: "Other Projects", projects: ungrouped });
    }
    return sections;
  }, []);

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

  // Load group sections when "Show Groups" is on (and not filtered to one group)
  const groupingActive = showGroups && !filterGroup;
  useEffect(() => {
    if (!groupingActive) {
      setGroupSections(null);
      return;
    }
    let cancelled = false;
    loadGroupSections(projects).then((sections) => {
      if (!cancelled) setGroupSections(sections);
    });
    return () => { cancelled = true; };
  }, [groupingActive, projects, loadGroupSections]);

  useEffect(() => {
    if (!groupingActive) return;
    const handler = () => {
      loadGroupSections(projects).then(setGroupSections);
    };
    window.addEventListener("groups-updated", handler);
    window.addEventListener("project-added", handler);
    window.addEventListener("project-removed", handler);
    return () => {
      window.removeEventListener("groups-updated", handler);
      window.removeEventListener("project-added", handler);
      window.removeEventListener("project-removed", handler);
    };
  }, [groupingActive, projects, loadGroupSections]);

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

  // Grouped view: same search + sort, but partitioned into sections
  const filteredSections: ProjectGroupSection[] =
    groupingActive && groupSections
      ? groupSections
          .map((section) => ({
            ...section,
            projects: sortNames(section.projects.filter(matchesSearch)),
          }))
          .filter((section) => section.projects.length > 0)
      : [];
  const groupsReady = !groupingActive || groupSections !== null;

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
                    <div className="w-7 h-7 rounded-md border border-border-strong/40 flex items-center justify-center shrink-0 text-text-muted/70">
                      <FolderOpen size={13} />
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
                    <span className="text-[11px] text-danger">Missing folder</span>
                  ) : !isConfigured ? (
                    <span className="text-[11px] text-warning/80">No agents</span>
                  ) : drift === true ? (
                    <span className="text-[11px] text-warning">Drifted</span>
                  ) : drift === false ? (
                    <span className="text-[11px] text-text-muted/50">Synced</span>
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
          {!filterGroup && (
            <button
              onClick={toggleShowGroups}
              aria-pressed={showGroups}
              title={showGroups ? "Show all projects in one list" : "Group projects by Project Group"}
              className={`flex h-7 items-center gap-1.5 rounded-md border px-2.5 text-[12px] font-medium transition-colors ${
                showGroups
                  ? "border-brand/40 bg-brand/15 text-brand"
                  : "border-border-strong/50 bg-bg-input text-text-muted hover:text-text-base"
              }`}
            >
              <Layers size={12} />
              Show Groups
            </button>
          )}
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
              className="flex h-7 items-center gap-1.5 rounded-md border border-border-strong/50 bg-transparent px-3 text-[12px] font-medium text-text-muted transition-colors hover:bg-bg-input hover:text-text-base disabled:cursor-not-allowed disabled:opacity-60"
            >
              <RefreshCw size={12} className={syncAllStatus === "syncing" ? "animate-spin" : ""} />
              {syncAllStatus === "syncing" ? "Syncing…" : "Sync all"}
            </button>
          )}
          <button
            onClick={onCreate}
            className="flex h-7 items-center gap-1.5 rounded bg-brand px-3 text-[12px] font-medium text-white shadow-sm transition-colors hover:bg-brand-hover"
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
          label={filterGroup === "__ungrouped__" ? "Other Projects" : filterGroup ? filterGroup : "Projects"}
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
        ) : groupingActive ? (
          !groupsReady ? null : filteredSections.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-16 text-center border border-border-strong/30 rounded-lg bg-bg-input/40">
              <p className="text-[13px] text-text-base mb-1">No matching projects</p>
              <p className="text-[12px] text-text-muted">Try another search term.</p>
            </div>
          ) : (
            <div className="space-y-8">
              {filteredSections.map((section) => (
                <section key={section.id} className="space-y-3">
                  <GroupSectionHeader
                    label={section.label}
                    projects={section.projects}
                    driftByProject={driftByProject}
                  />
                  {viewMode === "table"
                    ? renderTable(section.projects)
                    : renderCardGrid(section.projects)}
                </section>
              ))}
            </div>
          )
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
