import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AgentIcon } from "../../../../../components/AgentIcon";
import { ActivityFeed } from "../ActivityFeed";
import {
  ActivityCalendarGrid,
  ConfigBreakdown,
  Sparkline,
  SummaryCard,
  SyncRing,
  type ConfigItem,
} from "../SummaryCharts";
import {
  CALENDAR_WEEKS,
  STAT_WINDOW_DAYS,
  agentSyncRows,
  buildActivityCalendar,
  countRecent,
  dailyCounts,
  historyStart,
  latestSync,
  localDayKey,
  projectLevelDrift,
  summarizeSync,
  syncStatusLabel,
  truncatedWindowStart,
  type AgentSyncRow,
} from "../summaryStats";
import { relativeTime } from "../../helpers";
import type { ActivityEntry, AgentInfo, DriftReport, Project, ProjectFileInfo } from "../../types";

type ProjectTabId =
  | "summary" | "agents" | "commands" | "hooks" | "custom_agents" | "skills"
  | "mcp_servers" | "groups" | "project_file" | "rules"
  | "docs_files" | "docs_links" | "docs_notes" | "memory" | "activity"
  | "recommendations" | "tools" | "settings";

type DocEntry = { path: string; summary?: string };

interface SummaryPanelProps {
  project: Project;
  isCreating: boolean;
  /** Null while no memories are loaded for this project. */
  memories: Record<string, unknown> | null;
  projectFiles: ProjectFileInfo[];
  linkDocEntries: [string, DocEntry][];
  recsDisplayCount: number;
  activityEntries: ActivityEntry[];
  loadingActivity: boolean;
  activityLoadFailed: boolean;
  loadingMemories: boolean;
  driftReport: DriftReport | null;
  driftCheckFailed: boolean;
  availableAgents: AgentInfo[];
  selectTab: (tab: ProjectTabId) => void;
  updateField: <K extends keyof Project>(key: K, value: Project[K]) => void;
}

function shortDate(date: Date): string {
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** Today's key, updated when the local day changes under a panel left open. */
function useLocalDay(): string {
  const [day, setDay] = useState(() => localDayKey(new Date()));
  useEffect(() => {
    const timer = setInterval(() => setDay(localDayKey(new Date())), 60_000);
    return () => clearInterval(timer);
  }, []);
  return day;
}

function Placeholder({ className }: { className: string }) {
  return <div role="status" aria-label="Loading" className={`animate-pulse rounded bg-text-muted/15 ${className}`} />;
}

function configItem(
  key: string,
  count: number,
  singular: string,
  emptyLabel: string,
  colorClass: string,
  onView: () => void,
): ConfigItem {
  const countLabel = `${count} ${singular}${count === 1 ? "" : "s"}`;
  return { key, count, countLabel, emptyLabel, colorClass, onView };
}

const AGENT_STATUS_CLASSES: Record<AgentSyncRow["status"], string> = {
  synced: "text-success",
  drifted: "text-warning",
  checking: "text-text-muted",
  failed: "text-danger",
  not_set_up: "text-text-muted",
};

function AgentStatus({ row }: { row: AgentSyncRow }) {
  const text = row.status === "drifted" && row.driftedFiles > 0
    ? `${row.driftedFiles} ${row.driftedFiles === 1 ? "file" : "files"}`
    : syncStatusLabel(row.status);
  return <span className={`flex-shrink-0 text-[11px] ${AGENT_STATUS_CLASSES[row.status]}`}>{text}</span>;
}

function FooterLink({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" onClick={onClick} className="transition-colors hover:text-text-base">
      {children}
    </button>
  );
}

export function SummaryPanel({
  project,
  isCreating,
  memories,
  projectFiles,
  linkDocEntries,
  recsDisplayCount,
  activityEntries,
  loadingActivity,
  activityLoadFailed,
  loadingMemories,
  driftReport,
  driftCheckFailed,
  availableAgents,
  selectTab,
  updateField,
}: SummaryPanelProps) {
  const totalSkills = project.skills.length + (project.custom_skills?.length ?? 0);
  const totalRules = ((project.file_rules || {})["_project"] || []).length + (project.custom_rules?.length ?? 0);
  const totalSubAgents = (project.user_agents?.length ?? 0) + (project.custom_agents?.length ?? 0);
  const totalCommands = (project.user_commands?.length ?? 0) + (project.custom_commands?.length ?? 0);
  const totalHooks = project.hooks?.length ?? 0;
  const memoryCount = memories === null ? null : Object.keys(memories).length;
  const instructionFileCount = projectFiles.filter((file) => file.exists).length;
  const instructionText = instructionFileCount === 0
    ? "No instruction files"
    : `${instructionFileCount} instruction ${instructionFileCount === 1 ? "file" : "files"}`;
  const docsCount = linkDocEntries.length;
  const docsText = docsCount === 0
    ? "No docs links"
    : `${docsCount} docs ${docsCount === 1 ? "link" : "links"}`;
  const needsSetup = !isCreating && (!project.directory || project.agents.length === 0);

  const configItems: ConfigItem[] = [
    configItem("skills", totalSkills, "skill", "Skills", "bg-chart-skills", () => selectTab("skills")),
    configItem("rules", totalRules, "rule", "Rules", "bg-chart-rules", () => selectTab("rules")),
    configItem("mcp", project.mcp_servers.length, "MCP server", "MCP servers", "bg-chart-mcp", () => selectTab("mcp_servers")),
    configItem("agents", totalSubAgents, "agent", "Agents", "bg-chart-agents", () => selectTab("custom_agents")),
    configItem("commands", totalCommands, "command", "Commands", "bg-chart-commands", () => selectTab("commands")),
    configItem("hooks", totalHooks, "hook", "Hooks", "bg-chart-hooks", () => selectTab("hooks")),
  ];
  const configTotal = configItems.reduce((sum, item) => sum + item.count, 0);

  const localDay = useLocalDay();
  const stats = useMemo(() => {
    const now = new Date();
    return {
      calendar: buildActivityCalendar(activityEntries, now),
      sparkline: dailyCounts(activityEntries, STAT_WINDOW_DAYS, now),
      recentActivity: countRecent(activityEntries, STAT_WINDOW_DAYS, now),
      recentMemorySaves: countRecent(activityEntries, STAT_WINDOW_DAYS, now, "memory_stored"),
      windowStart: truncatedWindowStart(activityEntries, STAT_WINDOW_DAYS, now),
      lastSync: latestSync(activityEntries),
    };
    // `localDay` recomputes the windows when midnight passes.
  }, [activityEntries, localDay]); // eslint-disable-line react-hooks/exhaustive-deps
  const { calendar, sparkline, recentActivity, recentMemorySaves, windowStart, lastSync } = stats;
  const calendarRange = calendar.historySince === null
    ? `Last ${CALENDAR_WEEKS} weeks`
    : `Since ${shortDate(calendar.historySince)}`;
  const tileRange = windowStart === null ? undefined : `Since ${shortDate(windowStart)}`;

  const setUp = Boolean(project.directory) && project.agents.length > 0;
  const agentRows = agentSyncRows(
    project.agents,
    (id) => availableAgents.find((agent) => agent.id === id)?.label ?? id,
    driftReport,
    driftCheckFailed,
    setUp,
  );
  const sync = summarizeSync(agentRows, driftReport, driftCheckFailed, setUp);
  const projectDrift = setUp ? projectLevelDrift(driftReport) : [];

  let lastSyncText = "Not synced yet";
  if (activityLoadFailed) lastSyncText = "Last sync unavailable";
  else if (lastSync !== undefined) {
    lastSyncText = [lastSync.detail, relativeTime(lastSync.timestamp)].filter(Boolean).join(", ");
  } else if (historyStart(activityEntries) !== null) lastSyncText = "Last sync not in history";

  let memoryTotal: ReactNode = memoryCount;
  if (memoryCount === null) {
    if (loadingMemories) memoryTotal = <Placeholder className="h-[26px] w-10" />;
    else memoryTotal = isCreating ? 0 : "Unavailable";
  }
  const memorySavesText = `${recentMemorySaves} ${recentMemorySaves === 1 ? "save" : "saves"} ${windowStart === null ? `in ${STAT_WINDOW_DAYS} days` : `since ${shortDate(windowStart)}`}`;

  return (
    <div className="@container space-y-3">
      {/* Attention: setup gaps + recommendations */}
      {(needsSetup || recsDisplayCount > 0) && (
        <div className="space-y-3">
          {needsSetup && (
            <section className="rounded-lg border border-border-strong/35 bg-bg-input px-4 py-3">
              <h3 className="mb-1 text-[13px] font-medium text-text-base">Complete Setup</h3>
              <p className="mb-3 text-[12px] leading-relaxed text-text-muted">
                To start using this project, complete these steps:
              </p>
              <ol className="space-y-2 text-[12px] text-text-base">
                {!project.directory && (
                  <li className="flex items-start gap-2">
                    <span className="mt-0.5 w-4 shrink-0 tabular-nums text-text-muted/50">1.</span>
                    <div>
                      <button
                        type="button"
                        onClick={async () => {
                          let selected: string | null = null;
                          try {
                            selected = await invoke<string | null>("open_directory_dialog");
                          } catch (err) {
                            console.error("open_directory_dialog failed:", err);
                          }
                          if (selected) updateField("directory", selected);
                        }}
                        className="font-medium text-text-base transition-colors hover:text-brand"
                      >
                        Set project directory
                      </button>
                      <div className="mt-0.5 text-[11px] text-text-muted">
                        Click the path below the project name, or click here
                      </div>
                    </div>
                  </li>
                )}
                {project.agents.length === 0 && (
                  <li className="flex items-start gap-2">
                    <span className="mt-0.5 w-4 shrink-0 tabular-nums text-text-muted/50">
                      {!project.directory ? "2." : "1."}
                    </span>
                    <div>
                      <button
                        type="button"
                        onClick={() => selectTab("agents")}
                        className="font-medium text-text-base transition-colors hover:text-brand"
                      >
                        Add agent tools
                      </button>
                      <div className="mt-0.5 text-[11px] text-text-muted">
                        Select which agents will use this project
                      </div>
                    </div>
                  </li>
                )}
                <li className="flex items-start gap-2">
                  <span className="mt-0.5 w-4 shrink-0 text-text-muted/40">·</span>
                  <div>
                    <button
                      type="button"
                      onClick={() => selectTab("skills")}
                      className="text-text-base transition-colors hover:text-brand"
                    >
                      Add skills (optional)
                    </button>
                    <div className="mt-0.5 text-[11px] text-text-muted">
                      Give agents specialized capabilities
                    </div>
                  </div>
                </li>
              </ol>
            </section>
          )}

          {recsDisplayCount > 0 && (
            <div className="flex items-center gap-2 text-[12px] text-text-muted/60">
              <span>
                {recsDisplayCount === 1
                  ? "1 recommendation"
                  : `${recsDisplayCount} recommendations`}
              </span>
              <button
                type="button"
                onClick={() => selectTab("recommendations")}
                className="font-medium text-text-muted transition-colors hover:text-text-base"
              >
                Review
              </button>
            </div>
          )}
        </div>
      )}

      <div className="grid grid-cols-1 gap-3 @[520px]:grid-cols-3">
        <SummaryCard title="Sync status">
          <div className="flex items-center gap-3">
            <SyncRing status={sync.status} share={sync.share} />
            <div className="min-w-0">
              <div className="text-[15px] font-medium text-text-base">{sync.title}</div>
              {loadingActivity ? (
                <Placeholder className="mt-1 h-3 w-24" />
              ) : (
                <div title={lastSyncText} className="line-clamp-2 text-[12px] leading-snug text-text-muted">
                  {lastSyncText}
                </div>
              )}
            </div>
          </div>
        </SummaryCard>

        <SummaryCard title={`Activity, ${STAT_WINDOW_DAYS} days`} aside={tileRange}>
          {loadingActivity ? (
            <Placeholder className="h-[26px] w-10" />
          ) : activityLoadFailed ? (
            <div className="text-[12px] text-text-muted">Could not load activity.</div>
          ) : (
            <>
              <div className="text-[22px] font-medium leading-tight tabular-nums text-text-base">
                {recentActivity}
              </div>
              {recentActivity > 0 ? (
                <Sparkline values={sparkline} />
              ) : (
                <div className="mt-1 text-[12px] text-text-muted">No recent activity.</div>
              )}
            </>
          )}
        </SummaryCard>

        <SummaryCard title="Memories" aside={tileRange}>
          <div
            className={typeof memoryTotal === "string"
              ? "text-[12px] text-text-muted"
              : "text-[22px] font-medium leading-tight tabular-nums text-text-base"}
          >
            {memoryTotal}
          </div>
          {loadingActivity ? (
            <Placeholder className="mt-1.5 h-3 w-20" />
          ) : !activityLoadFailed && (
            <div className="mt-1 text-[12px] text-text-muted">{memorySavesText}</div>
          )}
        </SummaryCard>
      </div>

      <div className="grid grid-cols-1 gap-3 @[520px]:grid-cols-2">
        <SummaryCard title="Activity" aside={calendarRange}>
          {loadingActivity ? (
            <Placeholder className="h-[109px] w-full" />
          ) : activityLoadFailed ? (
            <p className="text-[12px] text-text-muted">Could not load activity.</p>
          ) : (
            <ActivityCalendarGrid key={project.name} days={calendar.days} rangeLabel={calendarRange} />
          )}
        </SummaryCard>
        <SummaryCard title="Configuration" aside={`${configTotal} ${configTotal === 1 ? "item" : "items"}`}>
          <ConfigBreakdown items={configItems} />
        </SummaryCard>
      </div>

      <div className="grid grid-cols-1 gap-3 @[520px]:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
        <ActivityFeed entries={activityEntries} loading={loadingActivity} failed={activityLoadFailed} />
        <SummaryCard title="Agents">
          {agentRows.length === 0 ? (
            <button
              type="button"
              onClick={() => selectTab("agents")}
              className="text-[12px] text-text-muted transition-colors hover:text-text-base"
            >
              Add agent tools
            </button>
          ) : (
            <div className="-my-1">
              {agentRows.map((row) => (
                <button
                  key={row.id}
                  type="button"
                  onClick={() => selectTab("agents")}
                  className="flex w-full items-center gap-2 py-1.5 text-left"
                >
                  <AgentIcon agentId={row.id} size={14} />
                  <span className="min-w-0 flex-1 truncate text-[12px] text-text-base">{row.label}</span>
                  <AgentStatus row={row} />
                </button>
              ))}
            </div>
          )}
          {projectDrift.length > 0 && (
            <p className="mt-2 border-t border-border-strong/20 pt-2 text-[11px] leading-relaxed text-warning">
              Project files also differ: {projectDrift.join(", ")}.
            </p>
          )}
        </SummaryCard>
      </div>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 px-1 text-[12px] text-text-muted">
        <FooterLink onClick={() => selectTab("project_file")}>{instructionText}</FooterLink>
        <FooterLink onClick={() => selectTab("docs_links")}>{docsText}</FooterLink>
        <FooterLink onClick={() => selectTab("memory")}>View memory</FooterLink>
      </div>
    </div>
  );
}
