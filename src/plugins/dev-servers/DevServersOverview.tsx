import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, ArrowUpRight, ExternalLink, Loader2, Play, Server as ServerIcon, Square, X } from "lucide-react";
import { openExternalUrl } from "../../lib/externalLinks";
import { formatServerUrlLabel, type DevServerStatus } from "./types";

const STATUS_POLL_MS = 3000;

interface DevServersOverviewProps {
  onNavigateToProject: (projectName: string) => void;
}

function groupByProject(statuses: DevServerStatus[]): Map<string, DevServerStatus[]> {
  const groups = new Map<string, DevServerStatus[]>();
  for (const status of statuses) {
    const existing = groups.get(status.project);
    if (existing) existing.push(status);
    else groups.set(status.project, [status]);
  }
  return groups;
}

export default function DevServersOverview({ onNavigateToProject }: DevServersOverviewProps) {
  const [statuses, setStatuses] = useState<DevServerStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyIds, setBusyIds] = useState<Set<string>>(new Set());

  const setBusy = (id: string, busy: boolean) => {
    setBusyIds((prev) => {
      const next = new Set(prev);
      if (busy) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const refresh = useCallback(async (showSpinner: boolean) => {
    if (showSpinner) setLoading(true);
    try {
      const result = await invoke<DevServerStatus[]>("list_dev_server_statuses", { project: undefined });
      setStatuses(result);
      setError(null);
    } catch (err) {
      setError(String(err));
    } finally {
      if (showSpinner) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh(true);
    const interval = setInterval(() => void refresh(false), STATUS_POLL_MS);
    return () => clearInterval(interval);
  }, [refresh]);

  const handleStart = async (status: DevServerStatus) => {
    setBusy(status.id, true);
    try {
      await invoke("start_dev_server", { project: status.project, id: status.id });
      await refresh(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(status.id, false);
    }
  };

  const handleStop = async (status: DevServerStatus) => {
    setBusy(status.id, true);
    try {
      await invoke("stop_dev_server", { id: status.id });
      await refresh(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(status.id, false);
    }
  };

  const groups = groupByProject(statuses);
  const runningCount = statuses.filter((s) => s.running).length;

  return (
    <div className="flex-1 h-full overflow-y-auto custom-scrollbar p-8 bg-bg-base">
      <div className="max-w-4xl mx-auto space-y-6">
        <div className="flex items-start gap-4">
          <div className="p-3 rounded-xl bg-brand/10 border border-brand/20 shrink-0">
            <ServerIcon size={20} className="text-brand" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold text-text-base mb-2">Servers</h1>
            <p className="text-text-muted text-[13px] leading-relaxed max-w-2xl">
              Every dev server configured across your projects, with its current status.
              {runningCount > 0 && ` ${runningCount} currently running.`}
            </p>
          </div>
        </div>

        {error && (
          <div className="flex items-center justify-between gap-3 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-[12px]">
            <div className="flex items-center gap-2">
              <AlertCircle size={13} className="shrink-0" />
              <span>{error}</span>
            </div>
            <button onClick={() => setError(null)} className="text-red-400/70 hover:text-red-400">
              <X size={13} />
            </button>
          </div>
        )}

        {loading ? (
          <div className="flex items-center justify-center py-16 text-text-muted text-[12px]">
            <Loader2 size={16} className="animate-spin mr-2" />
            Loading servers…
          </div>
        ) : groups.size === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center border border-dashed border-border-strong/40 rounded-xl">
            <ServerIcon size={24} className="text-text-muted mb-3" />
            <p className="text-[13px] text-text-base font-medium mb-1">No servers configured yet</p>
            <p className="text-[12px] text-text-muted max-w-sm">
              Open a project's Servers tab to add one.
            </p>
          </div>
        ) : (
          <div className="space-y-5">
            {Array.from(groups.entries()).map(([project, servers]) => (
              <div key={project} className="rounded-xl border border-border-strong/40 bg-bg-input overflow-hidden">
                <button
                  onClick={() => onNavigateToProject(project)}
                  className="w-full flex items-center justify-between gap-2 px-4 py-2.5 border-b border-border-strong/40 bg-bg-sidebar/40 hover:bg-bg-sidebar/70 transition-colors text-left"
                >
                  <span className="text-[12px] font-semibold text-text-base truncate">{project}</span>
                  <ArrowUpRight size={12} className="text-text-muted shrink-0" />
                </button>
                <div className="divide-y divide-border-strong/20">
                  {servers.map((status) => (
                    <div key={status.id} className="flex items-center gap-3 px-4 py-2.5">
                      <span
                        className={`w-2 h-2 rounded-full shrink-0 ${status.running ? "bg-green-400" : "bg-text-muted/40"}`}
                        aria-hidden="true"
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="text-[12.5px] font-medium text-text-base truncate">{status.name}</span>
                          <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-bg-sidebar text-text-muted shrink-0">
                            {status.package_manager}
                          </span>
                        </div>
                        <p className="text-[11px] text-text-muted truncate mt-0.5">
                          {status.package_manager} run {status.script}
                          {status.subdirectory ? ` — ${status.subdirectory}` : ""}
                        </p>
                      </div>
                      <span className={`text-[11px] shrink-0 ${status.running ? "text-success" : "text-text-muted"}`}>
                        {status.running ? "Running" : "Stopped"}
                      </span>
                      {status.running && status.urls && status.urls.length > 0 ? (
                        <div className="flex items-center gap-1.5 shrink-0">
                          {status.urls.map((url) => (
                            <button
                              key={url}
                              onClick={() => void openExternalUrl(url)}
                              className="flex items-center gap-1 text-[11px] text-brand hover:underline"
                              title={`Open ${url}`}
                            >
                              <ExternalLink size={11} />
                              {formatServerUrlLabel(url)}
                            </button>
                          ))}
                        </div>
                      ) : (
                        status.running &&
                        status.port && (
                          <button
                            onClick={() => void openExternalUrl(`http://localhost:${status.port}`)}
                            className="flex items-center gap-1 text-[11px] text-brand hover:underline shrink-0"
                            title={`Open http://localhost:${status.port}`}
                          >
                            <ExternalLink size={11} />
                            :{status.port}
                          </button>
                        )
                      )}
                      {busyIds.has(status.id) ? (
                        <div className="w-[26px] h-[26px] flex items-center justify-center text-text-muted shrink-0">
                          <Loader2 size={14} className="animate-spin" />
                        </div>
                      ) : status.running ? (
                        <button
                          onClick={() => handleStop(status)}
                          className="w-[26px] h-[26px] flex items-center justify-center rounded-md text-text-muted hover:bg-red-500/10 hover:text-red-400 transition-colors shrink-0"
                          title="Stop"
                          aria-label="Stop server"
                        >
                          <Square size={13} />
                        </button>
                      ) : (
                        <button
                          onClick={() => handleStart(status)}
                          className="w-[26px] h-[26px] flex items-center justify-center rounded-md text-text-muted hover:bg-green-500/10 hover:text-green-400 transition-colors shrink-0"
                          title="Start"
                          aria-label="Start server"
                        >
                          <Play size={13} />
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
