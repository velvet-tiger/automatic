import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  Loader2,
  Pencil,
  Play,
  Plus,
  Server as ServerIcon,
  Square,
  Trash2,
  X,
} from "lucide-react";
import { openExternalUrl } from "../../lib/externalLinks";
import {
  formatServerUrlLabel,
  PACKAGE_MANAGERS,
  type DevServerStatus,
  type LogLine,
  type NpmScriptEntry,
  type PackageManager,
  type ServerConfig,
} from "./types";

const STATUS_POLL_MS = 2000;
const LOG_POLL_MS = 1500;

interface ServersPanelProps {
  projectName: string;
  projectDirectory: string;
}

interface FormState {
  id: string;
  name: string;
  packageManager: PackageManager;
  script: string;
  subdirectory: string;
  port: string;
}

const EMPTY_FORM: FormState = {
  id: "",
  name: "",
  packageManager: "npm",
  script: "",
  subdirectory: "",
  port: "",
};

function statusFor(statuses: DevServerStatus[], id: string): DevServerStatus | undefined {
  return statuses.find((s) => s.id === id);
}

export default function ServersPanel({ projectName, projectDirectory }: ServersPanelProps) {
  const [configs, setConfigs] = useState<ServerConfig[]>([]);
  const [statuses, setStatuses] = useState<DevServerStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyIds, setBusyIds] = useState<Set<string>>(new Set());
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [logLines, setLogLines] = useState<LogLine[]>([]);
  const [form, setForm] = useState<FormState | null>(null);
  const [detectedScripts, setDetectedScripts] = useState<NpmScriptEntry[]>([]);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const logEndRef = useRef<HTMLDivElement>(null);

  // Quick-add suggestions shown in the empty state, detected from the
  // project root's package.json. Only relevant while no servers are
  // configured yet — once one exists, the suggestions are no longer shown.
  const [suggestedScripts, setSuggestedScripts] = useState<NpmScriptEntry[]>([]);
  const [suggestedPackageManager, setSuggestedPackageManager] = useState<PackageManager>("npm");
  const [addingScript, setAddingScript] = useState<string | null>(null);

  const setBusy = (id: string, busy: boolean) => {
    setBusyIds((prev) => {
      const next = new Set(prev);
      if (busy) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const loadConfigs = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<ServerConfig[]>("list_dev_server_configs", { project: projectName });
      setConfigs(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [projectName]);

  const refreshStatuses = useCallback(async () => {
    try {
      const result = await invoke<DevServerStatus[]>("list_dev_server_statuses", { project: projectName });
      setStatuses(result);
    } catch (err) {
      console.error("Failed to refresh dev server statuses:", err);
    }
  }, [projectName]);

  useEffect(() => {
    void loadConfigs();
    void refreshStatuses();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectName]);

  useEffect(() => {
    const interval = setInterval(() => void refreshStatuses(), STATUS_POLL_MS);
    return () => clearInterval(interval);
  }, [refreshStatuses]);

  useEffect(() => {
    if (!expandedId) {
      setLogLines([]);
      return;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const lines = await invoke<LogLine[]>("get_dev_server_log", { id: expandedId });
        if (!cancelled) setLogLines(lines);
      } catch (err) {
        console.error("Failed to load dev server log:", err);
      }
    };
    void load();
    const interval = setInterval(load, LOG_POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [expandedId]);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ block: "end" });
  }, [logLines]);

  useEffect(() => {
    if (loading || configs.length > 0) {
      setSuggestedScripts([]);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const [pm, scripts] = await Promise.all([
          invoke<PackageManager | null>("detect_dev_server_package_manager", {
            projectDir: projectDirectory,
            subdirectory: undefined,
          }),
          invoke<NpmScriptEntry[]>("list_dev_server_scripts", {
            projectDir: projectDirectory,
            subdirectory: undefined,
          }).catch(() => []),
        ]);
        if (cancelled) return;
        setSuggestedPackageManager(pm ?? "npm");
        setSuggestedScripts(scripts);
      } catch {
        if (!cancelled) setSuggestedScripts([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loading, configs.length, projectDirectory]);

  const handleQuickAdd = async (script: NpmScriptEntry) => {
    setAddingScript(script.name);
    setError(null);
    try {
      const payload: ServerConfig = {
        id: "",
        name: script.name,
        package_manager: suggestedPackageManager,
        script: script.name,
        subdirectory: "",
        port: null,
        created_at: "",
      };
      await invoke("save_dev_server_config", { project: projectName, config: payload });
      await loadConfigs();
    } catch (err) {
      setError(String(err));
    } finally {
      setAddingScript(null);
    }
  };

  const handleStart = async (config: ServerConfig) => {
    setBusy(config.id, true);
    setError(null);
    try {
      await invoke("start_dev_server", { project: projectName, id: config.id });
      await refreshStatuses();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(config.id, false);
    }
  };

  const handleStop = async (config: ServerConfig) => {
    setBusy(config.id, true);
    setError(null);
    try {
      await invoke("stop_dev_server", { id: config.id });
      await refreshStatuses();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(config.id, false);
    }
  };

  const handleDelete = async (config: ServerConfig) => {
    const confirmed = await ask(`Delete "${config.name}"? This cannot be undone.`, {
      title: "Delete server",
      kind: "warning",
    });
    if (!confirmed) return;

    setBusy(config.id, true);
    setError(null);
    try {
      await invoke("delete_dev_server_config", { project: projectName, id: config.id });
      if (expandedId === config.id) setExpandedId(null);
      await loadConfigs();
      await refreshStatuses();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(config.id, false);
    }
  };

  const runDetection = useCallback(
    async (subdirectory: string) => {
      try {
        const [pm, scripts] = await Promise.all([
          invoke<PackageManager | null>("detect_dev_server_package_manager", {
            projectDir: projectDirectory,
            subdirectory: subdirectory || undefined,
          }),
          invoke<NpmScriptEntry[]>("list_dev_server_scripts", {
            projectDir: projectDirectory,
            subdirectory: subdirectory || undefined,
          }).catch(() => []),
        ]);
        setDetectedScripts(scripts);
        if (pm) {
          setForm((prev) => (prev ? { ...prev, packageManager: pm } : prev));
        }
      } catch {
        setDetectedScripts([]);
      }
    },
    [projectDirectory]
  );

  const openCreateForm = () => {
    setFormError(null);
    setForm({ ...EMPTY_FORM });
    setDetectedScripts([]);
    void runDetection("");
  };

  const openEditForm = (config: ServerConfig) => {
    setFormError(null);
    setForm({
      id: config.id,
      name: config.name,
      packageManager: config.package_manager,
      script: config.script,
      subdirectory: config.subdirectory,
      port: config.port ? String(config.port) : "",
    });
    setDetectedScripts([]);
    void runDetection(config.subdirectory);
  };

  const closeForm = () => {
    setForm(null);
    setFormError(null);
  };

  const handleSubdirectoryBlur = () => {
    if (form) void runDetection(form.subdirectory.trim());
  };

  const handleSave = async () => {
    if (!form) return;
    const name = form.name.trim();
    const script = form.script.trim();
    if (!name) {
      setFormError("Name is required.");
      return;
    }
    if (!script) {
      setFormError("Script is required.");
      return;
    }
    let port: number | null = null;
    if (form.port.trim()) {
      const parsed = Number(form.port.trim());
      if (!Number.isInteger(parsed) || parsed <= 0 || parsed > 65535) {
        setFormError("Port must be a whole number between 1 and 65535.");
        return;
      }
      port = parsed;
    }

    setSaving(true);
    setFormError(null);
    try {
      const payload: ServerConfig = {
        id: form.id,
        name,
        package_manager: form.packageManager,
        script,
        subdirectory: form.subdirectory.trim(),
        port,
        created_at: "",
      };
      await invoke("save_dev_server_config", { project: projectName, config: payload });
      closeForm();
      await loadConfigs();
    } catch (err) {
      setFormError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const rows = useMemo(
    () =>
      configs.map((config) => ({
        config,
        status: statusFor(statuses, config.id),
      })),
    [configs, statuses]
  );

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex items-start justify-between gap-4 px-6 py-5 border-b border-border-strong/40 shrink-0">
        <div className="flex items-start gap-3">
          <div className="p-2.5 rounded-lg bg-brand/10 border border-brand/20 shrink-0">
            <ServerIcon size={18} className="text-brand" />
          </div>
          <div>
            <h2 className="text-[15px] font-semibold text-text-base">Servers</h2>
            <p className="text-[12px] text-text-muted mt-0.5 max-w-xl">
              Start, stop, and monitor npm, pnpm, and yarn dev servers for this project.
            </p>
          </div>
        </div>
        <button
          onClick={openCreateForm}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-brand text-white hover:bg-brand/90 transition-colors shrink-0"
        >
          <Plus size={13} />
          New Server
        </button>
      </div>

      {error && (
        <div className="mx-6 mt-4 flex items-center justify-between gap-3 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-[12px] shrink-0">
          <div className="flex items-center gap-2">
            <AlertCircle size={13} className="shrink-0" />
            <span>{error}</span>
          </div>
          <button onClick={() => setError(null)} className="text-red-400/70 hover:text-red-400">
            <X size={13} />
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto custom-scrollbar p-6">
        {loading ? (
          <div className="flex items-center justify-center py-16 text-text-muted text-[12px]">
            <Loader2 size={16} className="animate-spin mr-2" />
            Loading servers…
          </div>
        ) : rows.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <ServerIcon size={24} className="text-text-muted mb-3" />
            <p className="text-[13px] text-text-base font-medium mb-1">No servers configured</p>
            <p className="text-[12px] text-text-muted max-w-sm">
              Add a server to run an npm, pnpm, or yarn script for this project and monitor its output here.
            </p>
            {suggestedScripts.length > 0 && (
              <div className="mt-5 w-full max-w-sm">
                <p className="text-[11px] text-text-muted mb-2">
                  Detected in package.json — add as a server:
                </p>
                <div className="flex flex-wrap justify-center gap-1.5">
                  {suggestedScripts.map((s) => (
                    <button
                      key={s.name}
                      onClick={() => void handleQuickAdd(s)}
                      disabled={addingScript !== null}
                      title={s.command}
                      className="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[11px] border border-border-strong/50 bg-bg-input text-text-base hover:border-brand/50 hover:text-brand transition-colors disabled:opacity-50 disabled:pointer-events-none"
                    >
                      {addingScript === s.name ? (
                        <Loader2 size={11} className="animate-spin" />
                      ) : (
                        <Plus size={11} />
                      )}
                      {s.name}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="space-y-2">
            {rows.map(({ config, status }) => (
              <ServerRow
                key={config.id}
                config={config}
                status={status}
                busy={busyIds.has(config.id)}
                expanded={expandedId === config.id}
                logLines={expandedId === config.id ? logLines : []}
                logEndRef={logEndRef}
                onToggleExpand={() => setExpandedId((prev) => (prev === config.id ? null : config.id))}
                onStart={() => handleStart(config)}
                onStop={() => handleStop(config)}
                onEdit={() => openEditForm(config)}
                onDelete={() => handleDelete(config)}
              />
            ))}
          </div>
        )}
      </div>

      {form && (
        <ServerFormDialog
          form={form}
          setForm={setForm}
          detectedScripts={detectedScripts}
          error={formError}
          saving={saving}
          isEditing={!!form.id}
          onSubdirectoryBlur={handleSubdirectoryBlur}
          onCancel={closeForm}
          onSave={handleSave}
        />
      )}
    </div>
  );
}

// ── Server row ──────────────────────────────────────────────────────────────

interface ServerRowProps {
  config: ServerConfig;
  status?: DevServerStatus;
  busy: boolean;
  expanded: boolean;
  logLines: LogLine[];
  logEndRef: React.RefObject<HTMLDivElement | null>;
  onToggleExpand: () => void;
  onStart: () => void;
  onStop: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

function ServerRow({
  config,
  status,
  busy,
  expanded,
  logLines,
  logEndRef,
  onToggleExpand,
  onStart,
  onStop,
  onEdit,
  onDelete,
}: ServerRowProps) {
  const running = status?.running ?? false;

  return (
    <div className="rounded-lg border border-border-strong/40 bg-bg-input overflow-hidden">
      <div className="flex items-center gap-3 px-4 py-3">
        <button
          onClick={onToggleExpand}
          className="text-text-muted hover:text-text-base transition-colors shrink-0"
          aria-label={expanded ? "Collapse log" : "Expand log"}
        >
          {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>

        <span
          className={`w-2 h-2 rounded-full shrink-0 ${running ? "bg-green-400" : "bg-text-muted/40"}`}
          aria-hidden="true"
        />

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[13px] font-medium text-text-base truncate">{config.name}</span>
            <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-bg-sidebar text-text-muted shrink-0">
              {config.package_manager}
            </span>
          </div>
          <p className="text-[11px] text-text-muted truncate mt-0.5">
            {config.package_manager} run {config.script}
            {config.subdirectory ? ` — ${config.subdirectory}` : ""}
            {running && status?.pid ? ` — pid ${status.pid}` : ""}
          </p>
        </div>

        <span className={`text-[11px] shrink-0 ${running ? "text-success" : "text-text-muted"}`}>
          {running ? "Running" : "Stopped"}
        </span>

        {running && status?.urls && status.urls.length > 0 ? (
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
          running &&
          config.port && (
            <button
              onClick={() => void openExternalUrl(`http://localhost:${config.port}`)}
              className="flex items-center gap-1 text-[11px] text-brand hover:underline shrink-0"
              title={`Open http://localhost:${config.port}`}
            >
              <ExternalLink size={11} />
              :{config.port}
            </button>
          )
        )}

        <div className="flex items-center gap-1 shrink-0">
          {busy ? (
            <div className="w-[26px] h-[26px] flex items-center justify-center text-text-muted">
              <Loader2 size={14} className="animate-spin" />
            </div>
          ) : running ? (
            <button
              onClick={onStop}
              className="w-[26px] h-[26px] flex items-center justify-center rounded-md text-text-muted hover:bg-red-500/10 hover:text-red-400 transition-colors"
              title="Stop"
              aria-label="Stop server"
            >
              <Square size={13} />
            </button>
          ) : (
            <button
              onClick={onStart}
              className="w-[26px] h-[26px] flex items-center justify-center rounded-md text-text-muted hover:bg-green-500/10 hover:text-green-400 transition-colors"
              title="Start"
              aria-label="Start server"
            >
              <Play size={13} />
            </button>
          )}
          <button
            onClick={onEdit}
            disabled={running}
            className="w-[26px] h-[26px] flex items-center justify-center rounded-md text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors disabled:opacity-30 disabled:pointer-events-none"
            title={running ? "Stop the server to edit it" : "Edit"}
            aria-label="Edit server"
          >
            <Pencil size={12} />
          </button>
          <button
            onClick={onDelete}
            disabled={running || busy}
            className="w-[26px] h-[26px] flex items-center justify-center rounded-md text-text-muted hover:bg-red-500/10 hover:text-red-400 transition-colors disabled:opacity-30 disabled:pointer-events-none"
            title={running ? "Stop the server to delete it" : "Delete"}
            aria-label="Delete server"
          >
            <Trash2 size={12} />
          </button>
        </div>
      </div>

      {expanded && (
        <div className="border-t border-border-strong/40 bg-bg-base">
          <div className="max-h-64 overflow-y-auto custom-scrollbar px-4 py-3 font-mono text-[11px] leading-relaxed">
            {logLines.length === 0 ? (
              <p className="text-text-muted">No output captured yet.</p>
            ) : (
              logLines.map((line, idx) => (
                <div
                  key={idx}
                  className={line.stream === "stderr" ? "text-red-400" : "text-text-base"}
                >
                  {line.text}
                </div>
              ))
            )}
            <div ref={logEndRef} />
          </div>
        </div>
      )}
    </div>
  );
}

// ── Create / edit form ───────────────────────────────────────────────────────

interface ServerFormDialogProps {
  form: FormState;
  setForm: (updater: (prev: FormState | null) => FormState | null) => void;
  detectedScripts: NpmScriptEntry[];
  error: string | null;
  saving: boolean;
  isEditing: boolean;
  onSubdirectoryBlur: () => void;
  onCancel: () => void;
  onSave: () => void;
}

function ServerFormDialog({
  form,
  setForm,
  detectedScripts,
  error,
  saving,
  isEditing,
  onSubdirectoryBlur,
  onCancel,
  onSave,
}: ServerFormDialogProps) {
  const update = (patch: Partial<FormState>) => setForm((prev) => (prev ? { ...prev, ...patch } : prev));

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4">
      <div className="w-full max-w-md rounded-xl border border-border-strong/40 bg-bg-input shadow-xl">
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40">
          <h3 className="text-[14px] font-semibold text-text-base">
            {isEditing ? "Edit Server" : "New Server"}
          </h3>
          <button onClick={onCancel} className="text-text-muted hover:text-text-base transition-colors">
            <X size={15} />
          </button>
        </div>

        <div className="px-5 py-4 space-y-3.5">
          {error && (
            <div className="px-3 py-2 rounded-md bg-red-500/10 border border-red-500/20 text-red-400 text-[12px]">
              {error}
            </div>
          )}

          <div>
            <label className="block text-[11px] font-medium text-text-muted mb-1">Name</label>
            <input
              type="text"
              value={form.name}
              onChange={(e) => update({ name: e.target.value })}
              placeholder="web"
              className="w-full text-[12px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 py-1.5 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
            />
          </div>

          <div>
            <label className="block text-[11px] font-medium text-text-muted mb-1">Package manager</label>
            <div className="relative">
              <select
                value={form.packageManager}
                onChange={(e) => update({ packageManager: e.target.value as PackageManager })}
                className="w-full appearance-none text-[12px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 pr-7 py-1.5 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
              >
                {PACKAGE_MANAGERS.map((pm) => (
                  <option key={pm} value={pm}>
                    {pm}
                  </option>
                ))}
              </select>
              <ChevronDown size={12} className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted" />
            </div>
          </div>

          <div>
            <label className="block text-[11px] font-medium text-text-muted mb-1">Script</label>
            <input
              type="text"
              value={form.script}
              onChange={(e) => update({ script: e.target.value })}
              placeholder="dev"
              className="w-full text-[12px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 py-1.5 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
            />
            {detectedScripts.length > 0 && (
              <div className="flex flex-wrap gap-1.5 mt-2">
                {detectedScripts.map((s) => (
                  <button
                    key={s.name}
                    type="button"
                    onClick={() => update({ script: s.name })}
                    title={s.command}
                    className={`px-2 py-0.5 rounded text-[11px] border transition-colors ${
                      form.script === s.name
                        ? "bg-brand/15 border-brand/40 text-brand"
                        : "bg-bg-sidebar border-border-strong/40 text-text-muted hover:text-text-base"
                    }`}
                  >
                    {s.name}
                  </button>
                ))}
              </div>
            )}
          </div>

          <div>
            <label className="block text-[11px] font-medium text-text-muted mb-1">
              Subdirectory <span className="text-text-muted/60">(optional, for monorepos)</span>
            </label>
            <input
              type="text"
              value={form.subdirectory}
              onChange={(e) => update({ subdirectory: e.target.value })}
              onBlur={onSubdirectoryBlur}
              placeholder="apps/web"
              className="w-full text-[12px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 py-1.5 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
            />
          </div>

          <div>
            <label className="block text-[11px] font-medium text-text-muted mb-1">
              Port{" "}
              <span className="text-text-muted/60">
                (optional — fallback Open link, only used if a URL can't be detected from the server's output)
              </span>
            </label>
            <input
              type="text"
              inputMode="numeric"
              value={form.port}
              onChange={(e) => update({ port: e.target.value })}
              placeholder="5173"
              className="w-full text-[12px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 py-1.5 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
            />
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-4 border-t border-border-strong/40">
          <button
            onClick={onCancel}
            className="px-3 py-1.5 rounded-md text-[12px] font-medium text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onSave}
            disabled={saving}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-brand text-white hover:bg-brand/90 transition-colors disabled:opacity-60"
          >
            {saving && <Loader2 size={12} className="animate-spin" />}
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
