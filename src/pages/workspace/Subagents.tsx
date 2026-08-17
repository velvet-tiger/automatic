import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { LineNumberedTextarea } from "../../components/LineNumberedTextarea";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, X, Edit2, Check, MessagesSquare, Copy, FolderGit2, Search } from "lucide-react";
import { AuthorSection } from "../../components/AuthorPanel";
import { TokenPill } from "../../components/TokenPill";
import { AssetTable } from "../../components/AssetTable";
import { AssetDrawer } from "../../components/AssetDrawer";
import { BuiltInBadge, ReadOnlyBadge, LockCell } from "../../components/ProtectionBadge";
import { useBulkSelection } from "../../lib/useBulkSelection";
import {
  type AssetSecurityScanRecord,
  formatAssetScanResult,
  getAssetSecurityDismissButtonClass,
  getAssetSecurityNoticeClass,
  getAssetSecurityStatus,
  scanAssetContent,
  toAssetSecurityScanRecord,
  warningFindings,
} from "../../lib/assetSecurity";

interface SubagentEntry {
  id: string;
  name: string;
  source?: string; // "automatic" | "local" | "codex" | "github"
  author?: string; // "Automatic" | "You" | "OpenAI"
  source_repo?: string;
}

interface UserAgent {
  name: string;
  content: string;
}

interface ProjectRef {
  name: string;
  directory: string;
}

/** Default bundled agents have machine names starting with "automatic-".
 *  Codex OpenAI agents start with "codex-" and end with "-openai".
 */
const isBundledAgent = (id: string) => id.startsWith("automatic-");
const isCodexAgent = (id: string) => id.startsWith("codex-") && id.endsWith("-openai");
const isDeletable = (entry: SubagentEntry) => !isBundledAgent(entry.id) && !isCodexAgent(entry.id);

const originLabel = (entry: SubagentEntry): { label: string; className: string; title?: string } => {
  if (isBundledAgent(entry.id) || entry.source === "automatic") {
    return { label: "Automatic", className: "text-text-muted", title: "Bundled with Automatic" };
  }
  if (isCodexAgent(entry.id) || entry.source === "codex") {
    return { label: "OpenAI", className: "text-success", title: "Codex OpenAI agent" };
  }
  if (entry.source === "github" && entry.source_repo) {
    return { label: entry.source_repo, className: "text-success", title: `Installed from ${entry.source_repo}` };
  }
  return { label: "Local", className: "text-text-muted", title: "Created locally" };
};

const DEFAULT_AGENT_CONTENT = `---
name: my-agent
description: A specialized AI assistant.
tools: Read, Grep, Glob, Bash
model: inherit
---

You are a specialized AI assistant. Your purpose is to help with specific tasks.

When invoked:
1. Analyze the request carefully
2. Use the appropriate tools
3. Provide clear, actionable responses
`;

export default function Subagents() {
  const [agents, setAgents] = useState<SubagentEntry[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("user_agents", recentRefresh);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [displayName, setDisplayName] = useState("");
  const [agentContent, setAgentContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newMachineName, setNewMachineName] = useState("");
  const [newDisplayName, setNewDisplayName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [securityNotice, setSecurityNotice] = useState<string | null>(null);
  const [currentScan, setCurrentScan] = useState<AssetSecurityScanRecord | null>(null);
  const [referencingProjects, setReferencingProjects] = useState<ProjectRef[]>([]);
  const [search, setSearch] = useState("");
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkProgress, setBulkProgress] = useState<{ done: number; total: number } | null>(null);

  useEffect(() => {
    loadAgents();
  }, []);

  const loadAgents = async () => {
    try {
      const result: SubagentEntry[] = await invoke("get_subagents");
      setAgents(result.sort((a, b) => a.name.localeCompare(b.name)));
      setError(null);
    } catch (err: any) {
      setError(`Failed to load agents: ${err}`);
    }
  };

  const loadReferencingProjects = async (id: string) => {
    try {
      const refs: ProjectRef[] = await invoke("get_projects_referencing_subagent", { agentMachineName: id });
      setReferencingProjects(refs.sort((a, b) => a.name.localeCompare(b.name)));
    } catch (err: any) {
      console.error("Failed to load referencing projects:", err);
      setReferencingProjects([]);
    }
  };

  const loadAgent = async (id: string) => {
    try {
      const raw: string = await invoke("read_subagent", { machineName: id });
      const agent: UserAgent = JSON.parse(raw);
      const scan = await scanAssetContent("user_agent", agent.content);
      setSelectedId(id);
      setDisplayName(agent.name);
      setAgentContent(agent.content);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
      setCurrentScan(toAssetSecurityScanRecord(scan));
      setSecurityNotice(
        scan.findings.length > 0
          ? formatAssetScanResult(scan, "user agent", {
              blockedHeader: "Dangerous content found in user agent:",
            })
          : null,
      );
      await loadReferencingProjects(id);
    } catch (err: any) {
      setError(`Failed to read agent: ${err}`);
    }
  };

  const handleSave = async () => {
    if (isCreating) {
      const id = newMachineName.trim();
      const name = newDisplayName.trim();
      if (!id || !name) return;
      try {
        const scan = await scanAssetContent("user_agent", agentContent);
        if (scan.blocked) {
          setError(formatAssetScanResult(scan, "user agent"));
          setSecurityNotice(null);
          return;
        }
        const warnings = warningFindings(scan);
        await invoke("save_subagent", { machineName: id, name, content: agentContent });
        const newEntry: SubagentEntry = { id, name };
        setAgents(prev => [...prev.filter(a => a.id !== id), newEntry].sort((a, b) => a.name.localeCompare(b.name)));
        setIsCreating(false);
        setIsEditing(false);
        setSelectedId(id);
        setDisplayName(name);
        setReferencingProjects([]);
        setError(null);
        setCurrentScan(toAssetSecurityScanRecord(scan));
        setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "user agent") : null);
        setRecentRefresh(prev => prev + 1);
      } catch (err: any) {
        setError(`Failed to save agent: ${err}`);
      }
    } else if (selectedId) {
      try {
        const scan = await scanAssetContent("user_agent", agentContent);
        if (scan.blocked) {
          setError(formatAssetScanResult(scan, "user agent"));
          setSecurityNotice(null);
          return;
        }
        const warnings = warningFindings(scan);
        await invoke("save_subagent", { machineName: selectedId, name: displayName, content: agentContent });
        setIsEditing(false);
        setAgents(prev => prev.map(a => a.id === selectedId ? { ...a, name: displayName } : a).sort((a, b) => a.name.localeCompare(b.name)));
        setError(null);
        setCurrentScan(toAssetSecurityScanRecord(scan));
        setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "user agent") : null);
      } catch (err: any) {
        setError(`Failed to save agent: ${err}`);
      }
    }
  };

  const closeDrawer = () => {
    setSelectedId(null);
    setDisplayName("");
    setAgentContent("");
    setIsEditing(false);
    setIsCreating(false);
    setReferencingProjects([]);
    setError(null);
    setSecurityNotice(null);
    setCurrentScan(null);
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete agent "${id}"?`, { title: "Delete Agent", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_subagent", { machineName: id });
      if (selectedId === id) closeDrawer();
      await loadAgents();
      setError(null);
      setSecurityNotice(null);
      setCurrentScan(null);
    } catch (err: any) {
      setError(`Failed to delete agent: ${err}`);
    }
  };

  const handleBulkDelete = async () => {
    const targets = agents.filter(a => selection.selectedIds.has(a.id) && isDeletable(a));
    if (targets.length === 0) return;

    const preview = targets.slice(0, 10).map(t => `• ${t.name}`).join("\n");
    const overflow = targets.length > 10 ? `\n…and ${targets.length - 10} more.` : "";
    const message = `Delete ${targets.length} agent${targets.length === 1 ? "" : "s"}?\n\n${preview}${overflow}\n\nThis cannot be undone.`;
    const confirmed = await ask(message, { title: "Delete Agents", kind: "warning" });
    if (!confirmed) return;

    setBulkDeleting(true);
    setBulkProgress({ done: 0, total: targets.length });
    const failed: { id: string; error: string }[] = [];
    for (let i = 0; i < targets.length; i++) {
      const id = targets[i]!.id;
      try {
        await invoke("delete_subagent", { machineName: id });
      } catch (err: any) {
        failed.push({ id, error: String(err) });
      }
      setBulkProgress({ done: i + 1, total: targets.length });
    }

    if (selectedId && targets.some(t => t.id === selectedId)) {
      closeDrawer();
    }

    await loadAgents();
    selection.clearSelection();
    setBulkDeleting(false);
    setBulkProgress(null);
    if (failed.length > 0) {
      const detail = failed.slice(0, 5).map(f => `${f.id}: ${f.error}`).join("\n");
      const more = failed.length > 5 ? `\n…and ${failed.length - 5} more.` : "";
      setError(`Failed to delete ${failed.length} agent${failed.length === 1 ? "" : "s"}:\n${detail}${more}`);
    } else {
      setError(null);
    }
  };

  const startCreateNew = () => {
    setSelectedId(null);
    setDisplayName("");
    setAgentContent(DEFAULT_AGENT_CONTENT);
    setIsCreating(true);
    setIsEditing(true);
    setNewMachineName("");
    setNewDisplayName("");
    setReferencingProjects([]);
    setSecurityNotice(null);
    setCurrentScan(null);
  };

  const handleDuplicate = async (id: string) => {
    const strippedId = id.startsWith("automatic-") ? id.slice("automatic-".length) : id;
    const base = `${strippedId}-copy`;
    let candidate = base;
    let suffix = 2;
    while (agents.some(a => a.id === candidate)) {
      candidate = `${base}-${suffix}`;
      suffix++;
    }
    try {
      const raw: string = await invoke("read_subagent", { machineName: id });
      const agent: UserAgent = JSON.parse(raw);
      const dupName = agent.name;
      await invoke("save_subagent", { machineName: candidate, name: dupName, content: agent.content });
      const newEntry: SubagentEntry = { id: candidate, name: dupName };
      setAgents(prev => [...prev, newEntry].sort((a, b) => a.name.localeCompare(b.name)));
      await loadAgent(candidate);
      setIsEditing(true);
      setError(null);
      setSecurityNotice(null);
    } catch (err: any) {
      setError(`Failed to duplicate agent: ${err}`);
    }
  };

  const selectedEntry = agents.find(a => a.id === selectedId);
  const { label: scanStatusLabel, className: scanStatusClass } = getAssetSecurityStatus(currentScan, {
    blockedLabel: "Danger",
  });
  const scanTimestamp = currentScan
    ? new Date(currentScan.scanned_at).toLocaleString()
    : null;
  const securityNoticeToneClass = getAssetSecurityNoticeClass(currentScan);
  const securityDismissButtonClass = getAssetSecurityDismissButtonClass(currentScan);

  const searchLower = search.trim().toLowerCase();
  const filteredAgents = agents.filter(a =>
    !searchLower || a.name.toLowerCase().includes(searchLower) || a.id.toLowerCase().includes(searchLower)
  );

  const selection = useBulkSelection(filteredAgents, a => a.id, isDeletable);
  const drawerOpen = isCreating || !!selectedId;

  const renderTableRow = (entry: SubagentEntry) => {
    const isRowSelected = selection.isSelected(entry.id);
    const isFocused = selectedId === entry.id && !isCreating;
    const deletable = isDeletable(entry);
    const origin = originLabel(entry);
    return (
      <tr
        key={entry.id}
        onClick={() => loadAgent(entry.id)}
        className={`group cursor-pointer border-b border-border-strong/20 last:border-b-0 transition-colors ${
          isFocused ? "bg-bg-sidebar/60" : "hover:bg-bg-input/70"
        }`}
      >
        <td className="px-3 py-2 w-9" onClick={(e) => e.stopPropagation()}>
          {deletable ? (
            <input
              type="checkbox"
              checked={isRowSelected}
              onChange={() => selection.toggleSelected(entry.id)}
              aria-label={`Select ${entry.name}`}
              className="cursor-pointer accent-brand"
            />
          ) : (
            <LockCell
              tooltip={
                isCodexAgent(entry.id)
                  ? "Codex OpenAI agent — cannot be deleted. Duplicate to create a local copy."
                  : "Bundled agent provided by Automatic — cannot be deleted. Duplicate to create a local copy."
              }
            />
          )}
        </td>
        <td className="px-3 py-2 w-11">
          <div className="w-8 h-8 rounded-md bg-icon-agent/15 flex items-center justify-center flex-shrink-0">
            <MessagesSquare size={15} className="text-icon-agent" />
          </div>
        </td>
        <td className="px-3 py-2 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <div className="min-w-0">
              <div className="text-[13px] font-medium text-text-base truncate">{entry.name}</div>
              <div className="text-[10px] text-text-muted truncate font-mono">{entry.id}</div>
            </div>
            {recentIds.has(entry.id) && (
              <span className="shrink-0 px-1.5 py-0.5 rounded bg-brand/15 text-brand text-[9px] font-semibold uppercase tracking-wider">New</span>
            )}
          </div>
        </td>
        <td className="px-3 py-2">
          <span className={`inline-flex items-center text-[11px] ${origin.className} truncate max-w-[200px]`} title={origin.title}>
            {origin.label}
          </span>
        </td>
        <td className="px-3 py-2 w-16 text-right" onClick={(e) => e.stopPropagation()}>
          {deletable ? (
            <button
              onClick={(e) => handleDelete(entry.id, e)}
              className="opacity-0 group-hover:opacity-100 p-1 text-text-muted hover:text-danger rounded transition-all"
              title="Delete agent"
            >
              <X size={13} />
            </button>
          ) : null}
        </td>
      </tr>
    );
  };

  return (
    <div className="flex h-full w-full flex-col bg-bg-base">

      {/* ── Top Toolbar ──────────────────────────────────────────────────── */}
      <div className="shrink-0 border-b border-border-strong/40 bg-bg-input/40">
        <div className="flex items-center justify-between px-4 pt-3 pb-2 gap-3">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Sub-Agents
          </span>

          <div className="flex items-center gap-2 shrink-0">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              <input
                type="text"
                placeholder="Search agents…"
                value={search}
                onChange={e => setSearch(e.target.value)}
                className="w-56 h-7 pl-7 pr-7 rounded-md bg-bg-input border border-border-strong/50 hover:border-border-strong focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 text-[12px] text-text-base placeholder-text-muted/60 transition-colors"
              />
              {search && (
                <button
                  onClick={() => setSearch("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={11} />
                </button>
              )}
            </div>

            <button
              onClick={startCreateNew}
              className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-brand hover:bg-brand-hover text-white text-[12px] font-medium transition-colors"
              title="New Agent"
            >
              <Plus size={12} /> New Agent
            </button>
          </div>
        </div>

        {/* Selection action bar — appears whenever anything is selected */}
        {selection.totalSelected > 0 && (
          <div className="flex items-center justify-between px-4 py-2 border-t border-border-strong/30 bg-brand/5">
            <span className="text-[12px] text-text-base">
              {selection.totalSelected} agent{selection.totalSelected === 1 ? "" : "s"} selected
              {bulkProgress && (
                <span className="ml-2 text-text-muted">
                  · Deleting {bulkProgress.done}/{bulkProgress.total}…
                </span>
              )}
            </span>
            <div className="flex items-center gap-2">
              <button
                onClick={selection.clearSelection}
                disabled={bulkDeleting}
                className="h-7 px-2.5 rounded-md text-[12px] text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors disabled:opacity-50"
              >
                Clear selection
              </button>
              <button
                onClick={handleBulkDelete}
                disabled={bulkDeleting}
                className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-danger/90 hover:bg-danger text-white text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-wait"
              >
                <X size={12} /> Delete selected
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Error + security banners */}
      {error && (
        <div className="border-b border-red-300/80 bg-red-50 p-3 text-[13px] text-red-950 flex items-center justify-between shrink-0">
          <div className="whitespace-pre-wrap">{error}</div>
          <button
            onClick={() => setError(null)}
            className="text-red-900/70 hover:text-red-950 transition-colors"
          >
            <X size={14} />
          </button>
        </div>
      )}
      {securityNotice && (
        <div className={`${securityNoticeToneClass} p-3 text-[13px] border-b flex items-center justify-between shrink-0`}>
          <div className="whitespace-pre-wrap">{securityNotice}</div>
          <button
            onClick={() => setSecurityNotice(null)}
            className={securityDismissButtonClass}
          >
            <X size={14} />
          </button>
        </div>
      )}

      {/* ── Table ────────────────────────────────────────────────────────── */}
      <AssetTable
        items={filteredAgents}
        getId={a => a.id}
        isEmpty={agents.length === 0}
        emptyState={
          <>
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-icon-agent/12 border border-icon-agent/20 flex items-center justify-center">
              <MessagesSquare size={22} className="text-icon-agent" strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">No agents yet</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              Sub-agents are specialized AI assistants that run in their own context window. Create agents for code review, debugging, planning tasks, and more.
            </p>
            <button
              onClick={startCreateNew}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
            >
              <Plus size={14} /> New Agent
            </button>
          </>
        }
        noMatchState={
          <p className="text-[13px] text-text-muted">
            {searchLower ? `No agents match "${search}".` : "No agents yet."}
          </p>
        }
        columns={[
          { key: "icon", header: "", className: "w-11" },
          { key: "name", header: "Name" },
          { key: "origin", header: "Origin" },
          { key: "actions", header: "", className: "w-16" },
        ]}
        renderRow={renderTableRow}
        selection={{
          allSelected: selection.allSelected,
          someSelected: selection.someSelected,
          disabled: selection.deletableItems.length === 0,
          onToggleAll: selection.toggleSelectAllVisible,
          ariaLabel: "Select all visible deletable agents",
        }}
        recentIds={recentIds}
      />

      {/* ── Drawer ───────────────────────────────────────────────────────── */}
      <AssetDrawer open={drawerOpen} onClose={closeDrawer} isEditing={isEditing} closeButtonTopClassName="top-4">
        <div className="flex-1 flex flex-col h-full min-h-0">
          {/* Header */}
          <div className="min-h-[44px] pl-6 pr-10 border-b border-border-strong/40 flex justify-between items-center gap-4 py-2 flex-shrink-0">
            <div className="flex items-center gap-3 min-w-0 flex-1">
              <MessagesSquare size={14} className="text-icon-agent flex-shrink-0" />
              {isCreating ? (
                <div className="flex flex-col gap-1.5 min-w-0">
                  <input
                    type="text"
                    placeholder="Display Name"
                    value={newDisplayName}
                    onChange={(e) => setNewDisplayName(e.target.value)}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-72"
                  />
                  <div className="flex items-center gap-2">
                    <input
                      type="text"
                      placeholder="machine-name (lowercase, hyphens)"
                      value={newMachineName}
                      onChange={(e) => setNewMachineName(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ''))}
                      className="bg-transparent border-none outline-none text-[11px] text-text-muted placeholder-text-muted/40 font-mono w-72"
                    />
                  </div>
                </div>
              ) : isEditing ? (
                <div className="flex flex-col gap-0.5 min-w-0">
                  <input
                    type="text"
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-72"
                    placeholder="Display Name"
                  />
                  <span className="text-[10px] text-text-muted font-mono">{selectedId}</span>
                </div>
              ) : (
                <div className="flex flex-col gap-0.5 min-w-0">
                  <h3 className="text-[14px] font-medium text-text-base truncate">{selectedEntry?.name || displayName}</h3>
                  <span className="text-[10px] text-text-muted font-mono">{selectedId}</span>
                </div>
              )}
            </div>

            <div className="flex items-center gap-2 flex-shrink-0">
              <TokenPill text={agentContent} />
              {selectedId && isBundledAgent(selectedId) && !isEditing && <BuiltInBadge />}
              {selectedId && isCodexAgent(selectedId) && !isEditing && (
                <span className="text-[10px] font-semibold text-success tracking-wider uppercase px-2 py-1 rounded-full bg-success/10 border border-success/20">
                  OpenAI
                </span>
              )}
              {selectedId && (isBundledAgent(selectedId) || isCodexAgent(selectedId)) && !isEditing && (
                <ReadOnlyBadge
                  tooltip={isCodexAgent(selectedId) ? "Codex OpenAI agent — editing is disabled. Duplicate to create a local copy." : "Bundled agent provided by Automatic — editing is disabled. Duplicate to create a local copy."}
                />
              )}
              {!isEditing && selectedId && (
                <button
                  onClick={() => handleDuplicate(selectedId)}
                  className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                  title="Duplicate as a local, editable copy"
                >
                  <Copy size={12} /> Duplicate
                </button>
              )}
              {!isEditing && selectedId && !isBundledAgent(selectedId) && !isCodexAgent(selectedId) && (
                <button
                  onClick={() => setIsEditing(true)}
                  className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                >
                  <Edit2 size={12} /> Edit
                </button>
              )}
              {isEditing && (
                <>
                  {!isCreating && (
                    <button
                      onClick={() => {
                        setIsEditing(false);
                        if (selectedId) loadAgent(selectedId);
                      }}
                      className="px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                    >
                      Cancel
                    </button>
                  )}
                  <button
                    onClick={handleSave}
                    disabled={isCreating ? (!newMachineName.trim() || !newDisplayName.trim()) : false}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                  >
                    <Check size={12} /> Save
                  </button>
                </>
              )}
            </div>
          </div>

          {!isEditing && (
            <div className="px-6 py-2.5 border-b border-border-strong/40 flex items-center gap-2 shrink-0 bg-bg-input/20">
              <span className="text-[10px] font-semibold text-text-muted tracking-wider uppercase">
                Current Security Scan
              </span>
              <span className={`px-2 py-0.5 rounded-full border text-[11px] font-medium ${scanStatusClass}`}>
                {scanStatusLabel}
              </span>
              <span className="text-[11px] text-text-muted">
                {scanTimestamp ? scanTimestamp : "No scan yet"}
              </span>
            </div>
          )}

          {/* Body */}
          <div className="flex-1 min-h-0 flex flex-col">
            {isEditing ? (
              <LineNumberedTextarea
                value={agentContent}
                onChange={setAgentContent}
                className="flex-1"
                placeholder="Write your agent content here as Markdown with YAML frontmatter..."
              />
            ) : (
              <>
                <div className="flex-1 min-h-0 overflow-y-auto custom-scrollbar">
                  <div className="px-6 pt-4 pb-3 border-b border-border-strong/40">
                    <AuthorSection
                      descriptor={
                        selectedEntry?.source === "codex"
                          ? { type: "provider", name: "OpenAI", url: "https://openai.com" }
                          : selectedEntry?.source === "github" && selectedEntry.source_repo
                            ? { type: "github", repo: selectedEntry.source_repo }
                          : selectedEntry?.source === "automatic" || (selectedId && isBundledAgent(selectedId))
                            ? { type: "provider", name: "Automatic", url: "https://automatic.computer" }
                            : { type: "local" }
                      }
                    />
                  </div>
                  <div className="p-6 font-mono text-[13px] whitespace-pre-wrap text-text-base leading-relaxed">
                    {agentContent || <span className="text-text-muted italic">This agent is empty. Click edit to add content.</span>}
                  </div>
                </div>

                {/* Used by projects panel */}
                {!isCreating && referencingProjects.length > 0 && (
                  <div className="flex-shrink-0 border-t border-border-strong/40 px-6 py-4 bg-bg-input/30">
                    <div className="flex items-center gap-2 mb-3">
                      <FolderGit2 size={13} className="text-text-muted" />
                      <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                        Used in {referencingProjects.length} {referencingProjects.length === 1 ? "project" : "projects"}
                      </span>
                    </div>
                    <ul className="space-y-1.5 max-h-[108px] overflow-y-auto custom-scrollbar">
                      {referencingProjects.map(project => (
                        <li key={project.name} className="flex items-center justify-between gap-3 py-1">
                          <span className="text-[13px] text-text-base truncate">{project.name}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      </AssetDrawer>
    </div>
  );
}
