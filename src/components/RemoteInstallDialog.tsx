import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  X,
  Loader2,
  CheckCircle2,
  AlertTriangle,
  Download,
  Package,
  ClipboardList,
  Server,
  ScrollText,
  LayoutTemplate,
  Terminal,
  Bot,
} from "lucide-react";

// ── Types ────────────────────────────────────────────────────────────────────

interface ManifestResource {
  name: string;
  path: string;
  description: string;
}

interface SkillsJsonSkill {
  name: string;
  description?: string;
}

interface SkillsSection {
  skill_json?: string;
  entries: SkillsJsonSkill[];
}

interface AutomaticManifest {
  name: string;
  version: string;
  description: string;
  author?: { name: string; url?: string };
  skills?: SkillsSection;
  mcp_servers: ManifestResource[];
  rules: ManifestResource[];
  templates: ManifestResource[];
  commands: ManifestResource[];
  agents: ManifestResource[];
}

interface SelectedResources {
  skills: string[];
  mcp_servers: string[];
  rules: string[];
  templates: string[];
  commands: string[];
  agents: string[];
}

interface InstallResult {
  installed: Record<string, string[]>;
  skipped: string[];
  warnings: string[];
}

interface RemoteInstallDialogProps {
  isOpen: boolean;
  repo: string;
  gitRef?: string | null;
  directory?: string | null;
  onClose: () => void;
  onInstalled: () => void;
}

type DialogStatus = "loading" | "preview" | "installing" | "success" | "error";

// ── Helpers ──────────────────────────────────────────────────────────────────

function parseInvokeResult<T>(value: unknown): T {
  if (typeof value === "string") {
    return JSON.parse(value) as T;
  }
  return value as T;
}

interface ResourceGroup {
  key: keyof SelectedResources;
  label: string;
  icon: React.ComponentType<{ size?: number; className?: string }>;
  items: Array<{ name: string; description?: string }>;
}

function buildResourceGroups(manifest: AutomaticManifest): ResourceGroup[] {
  const groups: ResourceGroup[] = [];

  const skillEntries = manifest.skills?.entries ?? [];
  if (skillEntries.length > 0) {
    groups.push({
      key: "skills",
      label: "Skills",
      icon: ClipboardList,
      items: skillEntries.map((s) => ({ name: s.name, description: s.description })),
    });
  }

  const resourceTypes: Array<{
    key: keyof SelectedResources;
    label: string;
    icon: React.ComponentType<{ size?: number; className?: string }>;
    items: ManifestResource[];
  }> = [
    { key: "mcp_servers", label: "MCP Servers", icon: Server, items: manifest.mcp_servers ?? [] },
    { key: "rules", label: "Rules", icon: ScrollText, items: manifest.rules ?? [] },
    { key: "templates", label: "Templates", icon: LayoutTemplate, items: manifest.templates ?? [] },
    { key: "commands", label: "Commands", icon: Terminal, items: manifest.commands ?? [] },
    { key: "agents", label: "Agents", icon: Bot, items: manifest.agents ?? [] },
  ];

  for (const rt of resourceTypes) {
    if (rt.items.length > 0) {
      groups.push({
        key: rt.key,
        label: rt.label,
        icon: rt.icon,
        items: rt.items.map((r) => ({ name: r.name, description: r.description })),
      });
    }
  }

  return groups;
}

// ── Component ────────────────────────────────────────────────────────────────

export default function RemoteInstallDialog({
  isOpen,
  repo,
  gitRef,
  directory,
  onClose,
  onInstalled,
}: RemoteInstallDialogProps) {
  const [status, setStatus] = useState<DialogStatus>("loading");
  const [manifest, setManifest] = useState<AutomaticManifest | null>(null);
  const [groups, setGroups] = useState<ResourceGroup[]>([]);
  const [selected, setSelected] = useState<Record<string, Set<string>>>({});
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<InstallResult | null>(null);

  // Fetch manifest when dialog opens
  useEffect(() => {
    if (!isOpen) return;

    setStatus("loading");
    setError(null);
    setManifest(null);
    setResult(null);

    (async () => {
      try {
        const raw = await invoke("fetch_remote_source", {
          repo,
          gitRef: gitRef ?? null,
          dir: directory ?? null,
        });
        const m = parseInvokeResult<AutomaticManifest>(raw);
        setManifest(m);

        const g = buildResourceGroups(m);
        setGroups(g);

        // Select all by default
        const sel: Record<string, Set<string>> = {};
        for (const group of g) {
          sel[group.key] = new Set(group.items.map((i) => i.name));
        }
        setSelected(sel);
        setStatus("preview");
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
        setStatus("error");
      }
    })();
  }, [isOpen, repo, gitRef, directory]);

  const toggleItem = useCallback((groupKey: string, itemName: string) => {
    setSelected((prev) => {
      const next = { ...prev };
      const set = new Set(prev[groupKey] ?? []);
      if (set.has(itemName)) {
        set.delete(itemName);
      } else {
        set.add(itemName);
      }
      next[groupKey] = set;
      return next;
    });
  }, []);

  const toggleGroup = useCallback((groupKey: string, items: Array<{ name: string }>) => {
    setSelected((prev) => {
      const next = { ...prev };
      const set = prev[groupKey] ?? new Set<string>();
      const allSelected = items.every((i) => set.has(i.name));
      if (allSelected) {
        next[groupKey] = new Set();
      } else {
        next[groupKey] = new Set(items.map((i) => i.name));
      }
      return next;
    });
  }, []);

  const totalSelected = Object.values(selected).reduce((sum, set) => sum + set.size, 0);

  const handleInstall = async () => {
    setStatus("installing");
    setError(null);

    try {
      const sel: SelectedResources = {
        skills: Array.from(selected["skills"] ?? []),
        mcp_servers: Array.from(selected["mcp_servers"] ?? []),
        rules: Array.from(selected["rules"] ?? []),
        templates: Array.from(selected["templates"] ?? []),
        commands: Array.from(selected["commands"] ?? []),
        agents: Array.from(selected["agents"] ?? []),
      };

      const raw = await invoke("install_remote_source", {
        repo,
        selected: JSON.stringify(sel),
        dir: directory ?? null,
      });
      const r = parseInvokeResult<InstallResult>(raw);
      setResult(r);
      setStatus("success");
      onInstalled();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  };

  const handleClose = () => {
    setStatus("loading");
    setError(null);
    setManifest(null);
    setResult(null);
    setGroups([]);
    setSelected({});
    onClose();
  };

  if (!isOpen) return null;

  const totalInstalled = result
    ? Object.values(result.installed).reduce((sum, arr) => sum + arr.length, 0)
    : 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={handleClose} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-lg mx-4 max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40 shrink-0">
          <div className="flex items-center gap-2.5">
            <Package size={16} className="text-brand" />
            <h2 className="text-[15px] font-semibold text-text-base">Install Package</h2>
          </div>
          <button
            onClick={handleClose}
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Content */}
        <div className="px-5 py-4 overflow-y-auto flex-1">
          {/* Repo info */}
          <div className="mb-4">
            <span className="text-[13px] font-mono text-text-muted">{repo}</span>
            {gitRef && (
              <span className="ml-2 text-[11px] font-mono text-text-muted/60">@{gitRef}</span>
            )}
            {directory && (
              <span className="ml-2 text-[11px] font-mono text-text-muted/60">/{directory}</span>
            )}
          </div>

          {/* Loading */}
          {status === "loading" && (
            <div className="flex items-center justify-center py-8 gap-2 text-text-muted">
              <Loader2 size={16} className="animate-spin" />
              <span className="text-[13px]">Fetching package manifest...</span>
            </div>
          )}

          {/* Preview */}
          {status === "preview" && manifest && (
            <div className="space-y-4">
              <div>
                <h3 className="text-[14px] font-semibold text-text-base">{manifest.name}</h3>
                <p className="text-[12px] text-text-muted mt-0.5">
                  v{manifest.version}
                  {manifest.author?.name && <> by {manifest.author.name}</>}
                </p>
                {manifest.description && (
                  <p className="text-[13px] text-text-muted mt-2 leading-relaxed">
                    {manifest.description}
                  </p>
                )}
              </div>

              {groups.length === 0 ? (
                <div className="p-3 rounded-lg bg-bg-sidebar text-[13px] text-text-muted">
                  This package contains no installable resources.
                </div>
              ) : (
                <div className="space-y-3">
                  <p className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                    Resources to Install
                  </p>
                  {groups.map((group) => {
                    const GroupIcon = group.icon;
                    const groupSet = selected[group.key] ?? new Set<string>();
                    const allSelected = group.items.every((i) => groupSet.has(i.name));
                    return (
                      <div
                        key={group.key}
                        className="rounded-lg border border-border-strong/40 overflow-hidden"
                      >
                        <button
                          onClick={() => toggleGroup(group.key, group.items)}
                          className="w-full flex items-center gap-2.5 px-3 py-2 bg-bg-sidebar hover:bg-bg-sidebar/80 transition-colors"
                        >
                          <input
                            type="checkbox"
                            checked={allSelected}
                            readOnly
                            className="accent-brand"
                          />
                          <GroupIcon size={13} className="text-text-muted" />
                          <span className="text-[12px] font-medium text-text-base flex-1 text-left">
                            {group.label}
                          </span>
                          <span className="text-[11px] text-text-muted">
                            {groupSet.size}/{group.items.length}
                          </span>
                        </button>
                        <div className="divide-y divide-border-strong/20">
                          {group.items.map((item) => (
                            <label
                              key={item.name}
                              className="flex items-start gap-2.5 px-3 py-2 hover:bg-bg-sidebar/40 cursor-pointer transition-colors"
                            >
                              <input
                                type="checkbox"
                                checked={groupSet.has(item.name)}
                                onChange={() => toggleItem(group.key, item.name)}
                                className="accent-brand mt-0.5"
                              />
                              <div className="flex-1 min-w-0">
                                <span className="text-[12px] font-mono text-text-base">
                                  {item.name}
                                </span>
                                {item.description && (
                                  <p className="text-[11px] text-text-muted mt-0.5 truncate">
                                    {item.description}
                                  </p>
                                )}
                              </div>
                            </label>
                          ))}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {/* Installing */}
          {status === "installing" && (
            <div className="flex items-center justify-center py-8 gap-2 text-text-muted">
              <Loader2 size={16} className="animate-spin" />
              <span className="text-[13px]">Installing resources...</span>
            </div>
          )}

          {/* Success */}
          {status === "success" && result && (
            <div className="space-y-3">
              <div className="p-3 rounded-lg bg-success/10 border border-success/20">
                <div className="flex items-center gap-2 mb-2">
                  <CheckCircle2 size={14} className="text-success" />
                  <span className="text-[12px] font-medium text-success">
                    Successfully installed {totalInstalled} resource
                    {totalInstalled !== 1 ? "s" : ""}
                  </span>
                </div>
                {Object.entries(result.installed).map(([type, names]) =>
                  names.length > 0 ? (
                    <div key={type} className="mt-1">
                      <span className="text-[11px] font-semibold text-text-muted uppercase">
                        {type.replace(/_/g, " ")}
                      </span>
                      <ul className="mt-0.5 space-y-0.5">
                        {names.map((name) => (
                          <li key={name} className="text-[12px] font-mono text-text-muted ml-3">
                            {name}
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : null
                )}
              </div>
              {result.warnings.length > 0 && (
                <div className="p-3 rounded-lg bg-yellow-500/10 border border-yellow-500/20">
                  <div className="flex items-center gap-2 mb-1">
                    <AlertTriangle size={14} className="text-yellow-400" />
                    <span className="text-[12px] font-medium text-yellow-400">Warnings</span>
                  </div>
                  {result.warnings.map((w, i) => (
                    <p key={i} className="text-[11px] text-yellow-300 ml-5">
                      {w}
                    </p>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Error */}
          {error && (
            <div className="mt-4 p-3 rounded-lg bg-red-500/10 border border-red-500/20">
              <p className="text-[12px] text-red-400">{error}</p>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-5 py-3 border-t border-border-strong/40 shrink-0">
          {status === "preview" && (
            <button
              onClick={handleInstall}
              disabled={totalSelected === 0}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Download size={14} />
              Install {totalSelected > 0 ? `(${totalSelected})` : ""}
            </button>
          )}
          <button
            onClick={handleClose}
            className="px-4 py-2 text-[13px] font-medium text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded-lg transition-colors"
          >
            {status === "success" ? "Done" : "Cancel"}
          </button>
        </div>
      </div>
    </div>
  );
}
