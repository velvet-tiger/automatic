import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { handleExternalLinkClick } from "../../lib/externalLinks";
import {
  Wrench,
  ExternalLink,
  Terminal,
  FileText,
  Search,
  CheckCircle2,
  MinusCircle,
  Globe,
  Puzzle,
  FolderOpen,
  Save,
  X,
} from "lucide-react";

// ── Types ─────────────────────────────────────────────────────────────────────

type ToolKind = "cli" | "doc_gen" | "analyser" | "other";

interface ToolDefinition {
  name: string;
  display_name: string;
  description: string;
  url: string;
  github_repo?: string;
  kind: ToolKind;
  detect_binary?: string;
  detect_dir?: string;
  binary_path?: string;
  plugin_id?: string;
  created_at: string;
}

interface ToolEntry extends ToolDefinition {
  /** `true` = binary found on $PATH, `false` = not found, `null` = no detect_binary set */
  detected: boolean | null;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function kindLabel(kind: ToolKind): string {
  switch (kind) {
    case "cli":      return "CLI";
    case "doc_gen":  return "Doc Generator";
    case "analyser": return "Analyser";
    default:         return "Other";
  }
}

function kindIcon(kind: ToolKind, size = 14) {
  switch (kind) {
    case "cli":
    case "analyser":
      return <Terminal size={size} className="text-text-muted flex-shrink-0" />;
    case "doc_gen":
      return <FileText size={size} className="text-text-muted flex-shrink-0" />;
    default:
      return <Wrench size={size} className="text-text-muted flex-shrink-0" />;
  }
}

function githubOwnerFromRepo(repo: string): string {
  return repo.split("/")[0] ?? "";
}

function ToolAvatar({ tool, size = 32 }: { tool: ToolDefinition; size?: number }) {
  const [broken, setBroken] = useState(false);
  const owner = tool.github_repo ? githubOwnerFromRepo(tool.github_repo) : null;
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

function DetectionBadge({ detected }: { detected: boolean | null }) {
  if (detected === null) return null;
  if (detected) {
    return (
      <span className="flex items-center gap-1 text-[11px] text-green-400">
        <CheckCircle2 size={11} />
        Installed
      </span>
    );
  }
  return (
    <span className="flex items-center gap-1 text-[11px] text-text-muted">
      <MinusCircle size={11} />
      Not installed
    </span>
  );
}

// ── Main view ─────────────────────────────────────────────────────────────────

export default function Tools() {
  const [entries, setEntries] = useState<ToolEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  async function load() {
    setLoading(true);
    try {
      const data = await invoke<ToolEntry[]>("list_tools_with_detection");
      setEntries(data);
      if (selected && !data.find((e) => e.name === selected)) {
        setSelected(null);
      }
    } catch (err) {
      console.error("Failed to load tools:", err);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filtered = entries.filter((e) => {
    if (!query.trim()) return true;
    const q = query.toLowerCase();
    return (
      e.display_name.toLowerCase().includes(q) ||
      e.description.toLowerCase().includes(q) ||
      e.name.toLowerCase().includes(q)
    );
  });

  const selectedEntry = entries.find((e) => e.name === selected) ?? null;

  return (
    <div className="flex h-full overflow-hidden">
      {/* ── Left pane: list ── */}
      <div className="w-[260px] flex-shrink-0 border-r border-border-strong/40 flex flex-col">
        {/* Search */}
        <div className="px-3 pt-3 pb-2 border-b border-border-strong/30">
          <div className="relative">
            <Search
              size={12}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none"
            />
            <input
              className="w-full pl-7 pr-3 py-1.5 rounded-md bg-bg-input border border-border-strong/50 text-[12px] text-text-base placeholder-text-muted focus:outline-none focus:ring-1 focus:ring-brand/60"
              placeholder="Search tools…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
        </div>

        {/* List */}
        <div className="flex-1 overflow-y-auto py-1 custom-scrollbar">
          {loading ? (
            <div className="px-4 py-6 text-[12px] text-text-muted text-center">
              Loading…
            </div>
          ) : filtered.length === 0 ? (
            <div className="px-4 py-6 text-[12px] text-text-muted text-center">
              {query ? "No matching tools" : "No tools installed"}
            </div>
          ) : (
            filtered.map((entry) => (
              <button
                key={entry.name}
                className={`w-full flex items-center gap-2.5 px-3 py-2 text-left transition-colors ${
                  selected === entry.name
                    ? "bg-bg-sidebar text-text-base"
                    : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
                }`}
                onClick={() => setSelected((prev) => (prev === entry.name ? null : entry.name))}
              >
                <ToolAvatar tool={entry} size={24} />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] font-medium truncate">{entry.display_name}</div>
                  <div className="flex items-center gap-1.5 mt-0.5">
                    {kindIcon(entry.kind, 10)}
                    <span className="text-[11px] truncate">{kindLabel(entry.kind)}</span>
                    {entry.detected === true && (
                      <span className="ml-auto">
                        <CheckCircle2 size={10} className="text-green-400 flex-shrink-0" />
                      </span>
                    )}
                  </div>
                </div>
              </button>
            ))
          )}
        </div>
      </div>

      {/* ── Right pane: detail ── */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {selectedEntry ? (
          <ToolDetail entry={selectedEntry} onReload={load} />
        ) : (
          <EmptyState />
        )}
      </div>
    </div>
  );
}

// ── Detail pane ───────────────────────────────────────────────────────────────

function ToolDetail({ entry, onReload }: { entry: ToolEntry; onReload: () => Promise<void> }) {
  const [binaryPath, setBinaryPath] = useState(entry.binary_path ?? "");
  const [saving, setSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<string | null>(null);

  useEffect(() => {
    setBinaryPath(entry.binary_path ?? "");
    setSaveStatus(null);
  }, [entry.binary_path, entry.name]);

  const hasBinaryOverrideChanges = (binaryPath.trim() || "") !== (entry.binary_path ?? "");

  async function persistBinaryPath(nextBinaryPath: string | null) {
    setSaving(true);
    setSaveStatus(null);
    try {
      const { detected, ...definition } = entry;
      await invoke("save_tool", {
        name: entry.name,
        data: JSON.stringify({
          ...definition,
          binary_path: nextBinaryPath && nextBinaryPath.trim() ? nextBinaryPath.trim() : null,
        }),
      });
      setBinaryPath(nextBinaryPath ?? "");
      setSaveStatus("Saved.");
      await onReload();
    } catch (err) {
      setSaveStatus(`Failed to save: ${String(err)}`);
    } finally {
      setSaving(false);
    }
  }

  async function handleBrowseBinaryPath() {
    try {
      const selected = await open({ multiple: false, directory: false });
      if (selected && typeof selected === "string") {
        setBinaryPath(selected);
        setSaveStatus(null);
      }
    } catch (err) {
      setSaveStatus(`Failed to open picker: ${String(err)}`);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
      {/* Header */}
      <div className="flex items-start gap-3 mb-5">
        <ToolAvatar tool={entry} size={40} />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <h2 className="text-[15px] font-semibold text-text-base truncate">
              {entry.display_name}
            </h2>
            <span className="flex items-center gap-1 text-[11px] px-1.5 py-0.5 rounded bg-bg-input border border-border-strong/40 text-text-muted">
              {kindIcon(entry.kind, 10)}
              {kindLabel(entry.kind)}
            </span>
          </div>
          <div className="flex items-center gap-3 mt-1">
            <code className="text-[11px] text-text-muted font-mono">{entry.name}</code>
            <DetectionBadge detected={entry.detected} />
          </div>
        </div>
      </div>

      {/* Description */}
      {entry.description && (
        <p className="text-[13px] text-text-muted leading-relaxed mb-5">
          {entry.description}
        </p>
      )}

      {/* Metadata rows */}
      <div className="flex flex-col gap-2">
        <MetaRow
          label="URL"
          icon={<Globe size={13} className="text-text-muted flex-shrink-0" />}
        >
          <a
            href={entry.url}
            target="_blank"
            rel="noreferrer"
            onClick={handleExternalLinkClick(entry.url)}
            className="flex items-center gap-1 text-[13px] text-brand hover:underline truncate max-w-xs"
          >
            {entry.url}
            <ExternalLink size={11} className="flex-shrink-0" />
          </a>
        </MetaRow>

        {entry.github_repo && (
          <MetaRow
            label="GitHub"
            icon={<Globe size={13} className="text-text-muted flex-shrink-0" />}
          >
            <a
              href={`https://github.com/${entry.github_repo}`}
              target="_blank"
              rel="noreferrer"
              onClick={handleExternalLinkClick(`https://github.com/${entry.github_repo}`)}
              className="flex items-center gap-1 text-[13px] text-brand hover:underline"
            >
              {entry.github_repo}
              <ExternalLink size={11} className="flex-shrink-0" />
            </a>
          </MetaRow>
        )}

        {entry.detect_binary && (
          <MetaRow
            label="Detection"
            icon={<Terminal size={13} className="text-text-muted flex-shrink-0" />}
          >
            <div className="flex items-center gap-2">
              <code className="text-[12px] font-mono text-text-base">
                which {entry.detect_binary}
              </code>
              <DetectionBadge detected={entry.detected} />
            </div>
          </MetaRow>
        )}

        {entry.detect_binary && (
          <MetaRow
            label="Binary Path"
            icon={<Terminal size={13} className="text-text-muted flex-shrink-0" />}
          >
            <div className="space-y-2">
              <p className="text-[12px] text-text-muted leading-relaxed">
                Override the executable used for this tool. Useful for release builds that cannot see your shell PATH.
              </p>
              <input
                value={binaryPath}
                onChange={(e) => setBinaryPath(e.target.value)}
                placeholder="Use detected PATH by default"
                className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 text-[12px] text-text-base placeholder-text-muted focus:outline-none focus:ring-1 focus:ring-brand/60"
              />
              <div className="flex items-center gap-2 flex-wrap">
                <button
                  onClick={handleBrowseBinaryPath}
                  className="flex items-center gap-1.5 px-2.5 py-1.5 text-[12px] font-medium rounded-md border border-border-strong/40 text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors"
                >
                  <FolderOpen size={12} /> Browse
                </button>
                <button
                  onClick={() => persistBinaryPath(binaryPath.trim() || null)}
                  disabled={saving || !hasBinaryOverrideChanges}
                  className="flex items-center gap-1.5 px-2.5 py-1.5 text-[12px] font-medium rounded-md border border-brand/40 text-brand hover:bg-brand/10 transition-colors disabled:opacity-40"
                >
                  <Save size={12} /> {saving ? "Saving…" : "Save"}
                </button>
                <button
                  onClick={() => persistBinaryPath(null)}
                  disabled={saving || !entry.binary_path}
                  className="flex items-center gap-1.5 px-2.5 py-1.5 text-[12px] font-medium rounded-md border border-border-strong/40 text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors disabled:opacity-40"
                >
                  <X size={12} /> Clear
                </button>
              </div>
              {entry.binary_path && (
                <code className="block text-[11px] text-text-muted break-all">
                  Saved override: {entry.binary_path}
                </code>
              )}
              {saveStatus && (
                <p className="text-[12px] text-text-muted">{saveStatus}</p>
              )}
            </div>
          </MetaRow>
        )}

        {entry.plugin_id && (
          <MetaRow
            label="Plugin"
            icon={<Puzzle size={13} className="text-text-muted flex-shrink-0" />}
          >
            <span className="text-[12px] text-text-muted font-mono">{entry.plugin_id}</span>
          </MetaRow>
        )}
      </div>
    </div>
  );
}

function MetaRow({
  label,
  icon,
  children,
}: {
  label: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start gap-3 px-3 py-2.5 rounded-md bg-bg-input border border-border-strong/30">
      {icon}
      <div className="flex-1 min-w-0">
        <div className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-0.5">
          {label}
        </div>
        {children}
      </div>
    </div>
  );
}

// ── Empty state ───────────────────────────────────────────────────────────────

function EmptyState() {
  return (
    <div className="flex-1 flex flex-col items-center justify-center gap-4 text-center px-8">
      <Wrench size={36} className="text-text-muted opacity-40" />
      <div>
        <p className="text-[13px] font-medium text-text-base mb-1">No tool selected</p>
        <p className="text-[12px] text-text-muted leading-relaxed max-w-[300px]">
          Tools are installed by plugins. Install a plugin that declares a tool type and it will appear here.
        </p>
      </div>
    </div>
  );
}
