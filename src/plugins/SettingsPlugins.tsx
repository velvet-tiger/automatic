import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// ── Types ─────────────────────────────────────────────────────────────────────

type PluginCategory = "core" | "agents" | "integrations" | "experimental";

interface PluginEntry {
  id: string;
  name: string;
  description: string;
  version: string;
  category: PluginCategory;
  enabled_by_default: boolean;
  enabled: boolean;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

const CATEGORY_ORDER: PluginCategory[] = ["core", "agents", "integrations", "experimental"];

const CATEGORY_LABELS: Record<PluginCategory, string> = {
  core: "Core",
  agents: "Agents",
  integrations: "Integrations",
  experimental: "Experimental",
};

function groupByCategory(plugins: PluginEntry[]): Map<PluginCategory, PluginEntry[]> {
  const map = new Map<PluginCategory, PluginEntry[]>();
  for (const plugin of plugins) {
    const cat = plugin.category as PluginCategory;
    if (!map.has(cat)) map.set(cat, []);
    map.get(cat)!.push(plugin);
  }
  return map;
}

// ── Component ─────────────────────────────────────────────────────────────────

export default function SettingsPlugins() {
  const [plugins, setPlugins] = useState<PluginEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [toggling, setToggling] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      try {
        const result = await invoke<PluginEntry[]>("list_app_plugins");
        setPlugins(result);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    }
    load();
  }, []);

  async function handleToggle(plugin: PluginEntry) {
    if (toggling.has(plugin.id)) return;

    const newEnabled = !plugin.enabled;

    // Optimistic update
    setPlugins((prev) =>
      prev.map((p) => (p.id === plugin.id ? { ...p, enabled: newEnabled } : p))
    );
    setToggling((prev) => new Set([...prev, plugin.id]));

    try {
      await invoke("set_app_plugin_enabled", { id: plugin.id, enabled: newEnabled });
    } catch (e) {
      // Revert on error
      setPlugins((prev) =>
        prev.map((p) => (p.id === plugin.id ? { ...p, enabled: !newEnabled } : p))
      );
      setError(String(e));
    } finally {
      setToggling((prev) => {
        const next = new Set(prev);
        next.delete(plugin.id);
        return next;
      });
    }
  }

  if (loading) {
    return (
      <div className="text-[13px] text-text-muted py-4">Loading plugins...</div>
    );
  }

  if (error) {
    return (
      <div className="text-[13px] text-danger py-4">{error}</div>
    );
  }

  if (plugins.length === 0) {
    return (
      <div className="text-[13px] text-text-muted py-4">No plugins available.</div>
    );
  }

  const grouped = groupByCategory(plugins);
  const enabledCount = plugins.filter((p) => p.enabled).length;

  return (
    <div>
      <div className="flex items-baseline gap-2 mb-1">
        <h2 className="text-lg font-medium text-text-base">Plugins</h2>
        <span className="text-[12px] text-text-muted">
          {enabledCount} of {plugins.length} enabled
        </span>
      </div>
      <p className="text-[13px] text-text-muted mb-6">
        Enable or disable bundled features. Changes take effect immediately.
      </p>

      <div className="flex flex-col gap-6">
        {CATEGORY_ORDER.filter((cat) => grouped.has(cat)).map((cat) => (
          <section key={cat}>
            {/* Category header */}
            <div className="flex items-center gap-2 mb-2">
              <span className="text-[11px] font-semibold text-text-muted uppercase tracking-wider">
                {CATEGORY_LABELS[cat]}
              </span>
              <div className="flex-1 h-px bg-border-strong/30" />
            </div>

            {/* Plugin rows */}
            <div className="flex flex-col gap-1">
              {grouped.get(cat)!.map((plugin) => (
                <PluginRow
                  key={plugin.id}
                  plugin={plugin}
                  toggling={toggling.has(plugin.id)}
                  onToggle={handleToggle}
                />
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}

// ── PluginRow ─────────────────────────────────────────────────────────────────

interface PluginRowProps {
  plugin: PluginEntry;
  toggling: boolean;
  onToggle: (plugin: PluginEntry) => void;
}

function PluginRow({ plugin, toggling, onToggle }: PluginRowProps) {
  return (
    <div
      className={`flex items-center justify-between gap-4 px-4 py-3 rounded-lg border transition-all ${
        plugin.enabled
          ? "border-border-strong/40 bg-bg-input"
          : "border-border-strong/20 bg-bg-input-dark opacity-60"
      }`}
    >
      {/* Info */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-[13px] font-medium text-text-base leading-tight">
            {plugin.name}
          </span>
          <span className="text-[11px] text-text-muted font-mono">v{plugin.version}</span>
          {plugin.category === "experimental" && (
            <span className="text-[10px] font-semibold uppercase tracking-wide px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/30">
              Experimental
            </span>
          )}
        </div>
        <p className="text-[12px] text-text-muted mt-0.5 leading-relaxed">
          {plugin.description}
        </p>
      </div>

      {/* Toggle */}
      <button
        onClick={() => onToggle(plugin)}
        disabled={toggling}
        aria-pressed={plugin.enabled}
        aria-label={`${plugin.enabled ? "Disable" : "Enable"} ${plugin.name}`}
        className={`relative flex-shrink-0 w-10 h-5 rounded-full transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-brand/50 disabled:opacity-50 disabled:cursor-not-allowed ${
          plugin.enabled ? "bg-brand" : "bg-surface-active"
        }`}
      >
        <div
          className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-all ${
            plugin.enabled ? "left-5" : "left-0.5"
          }`}
        />
      </button>
    </div>
  );
}
