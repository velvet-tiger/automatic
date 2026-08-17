import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { LineNumberedTextarea } from "../../components/LineNumberedTextarea";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, X, Edit2, FileText, Check, ScrollText, RefreshCw, FolderGit2, Copy, Search } from "lucide-react";
import { ICONS } from "../../lib/icons";
import { AuthorSection, type AuthorDescriptor } from "../../components/AuthorPanel";
import { TokenPill } from "../../components/TokenPill";
import { AssetTable } from "../../components/AssetTable";
import { AssetDrawer } from "../../components/AssetDrawer";
import { BuiltInBadge, ReadOnlyBadge, LockCell } from "../../components/ProtectionBadge";
import { useBulkSelection } from "../../lib/useBulkSelection";
import {
  type AssetSecurityScanRecord,
  getAssetSecurityDismissButtonClass,
  getAssetSecurityNoticeClass,
  formatAssetScanResult,
  getAssetSecurityStatus,
  scanAssetContent,
  toAssetSecurityScanRecord,
  warningFindings,
} from "../../lib/assetSecurity";

interface RuleEntry {
  id: string;
  name: string;
  plugin_id?: string;
}

interface Rule {
  name: string;
  content: string;
  plugin_id?: string;
  _author?: AuthorDescriptor;
}

interface RuleProjectStatus {
  name: string;
  synced: boolean;
}

// Per-project sync state.
// "needs-sync" = rule has changed, project not yet updated (shown yellow).
// "syncing"    = update in progress.
// "synced"     = project is up to date with the current rule content (green).
// "error"      = last sync attempt failed.
type SyncState = "needs-sync" | "syncing" | "synced" | "error";

/** Default rules are those shipped with the app — machine names start with "automatic-". */
const isDefaultRule = (id: string) => id.startsWith("automatic-");
const isDeletable = (entry: RuleEntry) => !isDefaultRule(entry.id) && !entry.plugin_id;

const originLabel = (entry: RuleEntry): { label: string; className: string; title?: string } => {
  if (isDefaultRule(entry.id)) {
    return { label: "Automatic", className: "text-text-muted", title: "Default rule provided by Automatic" };
  }
  if (entry.plugin_id) {
    return { label: entry.plugin_id, className: "text-text-muted", title: `Provided by plugin ${entry.plugin_id}` };
  }
  return { label: "Local", className: "text-text-muted", title: "Created locally" };
};

export default function Rules() {
  const [rules, setRules] = useState<RuleEntry[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("rules", recentRefresh);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [displayName, setDisplayName] = useState("");
  const [ruleContent, setRuleContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newMachineName, setNewMachineName] = useState("");
  const [newDisplayName, setNewDisplayName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [securityNotice, setSecurityNotice] = useState<string | null>(null);
  const [currentScan, setCurrentScan] = useState<AssetSecurityScanRecord | null>(null);
  const [ruleAuthor, setRuleAuthor] = useState<AuthorDescriptor | null>(null);
  const [search, setSearch] = useState("");
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkProgress, setBulkProgress] = useState<{ done: number; total: number } | null>(null);

  // Projects referencing this rule
  const [referencingProjects, setReferencingProjects] = useState<string[]>([]);
  const [projectSyncState, setProjectSyncState] = useState<Record<string, SyncState>>({});
  const [syncAllState, setSyncAllState] = useState<SyncState>("needs-sync");

  useEffect(() => {
    loadRules();
  }, []);

  const loadRules = async () => {
    try {
      const result: RuleEntry[] = await invoke("get_rules");
      setRules(result.sort((a, b) => a.name.localeCompare(b.name)));
      setError(null);
    } catch (err: any) {
      setError(`Failed to load rules: ${err}`);
    }
  };

  // Load referencing projects with their actual on-disk sync status.
  const loadReferencingProjects = async (id: string) => {
    try {
      const statuses: RuleProjectStatus[] = await invoke("get_projects_referencing_rule", { ruleName: id });
      const sorted = statuses.sort((a, b) => a.name.localeCompare(b.name));
      setReferencingProjects(sorted.map(s => s.name));
      const initial: Record<string, SyncState> = {};
      for (const s of sorted) initial[s.name] = s.synced ? "synced" : "needs-sync";
      setProjectSyncState(initial);
      // Aggregate: all synced → "synced", otherwise "needs-sync".
      const allSynced = sorted.length > 0 && sorted.every(s => s.synced);
      setSyncAllState(allSynced ? "synced" : "needs-sync");
    } catch (err: any) {
      // Non-fatal — the usage panel just won't show.
      console.error("Failed to load referencing projects:", err);
      setReferencingProjects([]);
    }
  };

  const loadRule = async (id: string) => {
    try {
      const raw: string = await invoke("read_rule", { machineName: id });
      const rule: Rule = JSON.parse(raw);
      const scan = await scanAssetContent("rule", rule.content);
      setSelectedId(id);
      setDisplayName(rule.name);
      setRuleContent(rule.content);
      setRuleAuthor(rule._author ?? null);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
      setCurrentScan(toAssetSecurityScanRecord(scan));
      setSecurityNotice(
        scan.findings.length > 0
          ? formatAssetScanResult(scan, "rule", {
              blockedHeader: "Dangerous content found in rule:",
            })
          : null,
      );
      await loadReferencingProjects(id);
    } catch (err: any) {
      setError(`Failed to read rule: ${err}`);
    }
  };

  // Mark all referencing projects as needing a sync (called after saving the rule).
  const markAllNeedsSync = () => {
    setProjectSyncState(prev => {
      const next: Record<string, SyncState> = {};
      for (const p of Object.keys(prev)) next[p] = "needs-sync";
      return next;
    });
    setSyncAllState("needs-sync");
  };

  const handleSave = async () => {
    if (isCreating) {
      const id = newMachineName.trim();
      const name = newDisplayName.trim();
      if (!id || !name) return;
      try {
        const scan = await scanAssetContent("rule", ruleContent);
        if (scan.blocked) {
          setError(formatAssetScanResult(scan, "rule"));
          setSecurityNotice(null);
          return;
        }
        const warnings = warningFindings(scan);
        await invoke("save_rule", { machineName: id, name, content: ruleContent });
        // Insert into the list in-place (sorted), then select — no
        // loadRules() call so there is no async gap that could lose selection.
        const newEntry: RuleEntry = { id, name };
        setRules(prev =>
          [...prev.filter(r => r.id !== id), newEntry].sort((a, b) =>
            a.name.localeCompare(b.name)
          )
        );
        setIsCreating(false);
        setIsEditing(false);
        setSelectedId(id);
        setDisplayName(name);
        setRuleAuthor(null);
        setReferencingProjects([]);
        setProjectSyncState({});
        setSyncAllState("needs-sync");
        setError(null);
        setCurrentScan(toAssetSecurityScanRecord(scan));
        setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "rule") : null);
        setRecentRefresh(prev => prev + 1);
      } catch (err: any) {
        setError(`Failed to save rule: ${err}`);
      }
    } else if (selectedId) {
      try {
        const scan = await scanAssetContent("rule", ruleContent);
        if (scan.blocked) {
          setError(formatAssetScanResult(scan, "rule"));
          setSecurityNotice(null);
          return;
        }
        const warnings = warningFindings(scan);
        await invoke("save_rule", { machineName: selectedId, name: displayName, content: ruleContent });
        setIsEditing(false);
        // Update entry in-place — no loadRules so selection is preserved.
        setRules(prev =>
          prev
            .map(r => (r.id === selectedId ? { ...r, name: displayName } : r))
            .sort((a, b) => a.name.localeCompare(b.name))
        );
        // Rule content changed — all referencing projects need re-syncing.
        markAllNeedsSync();
        setError(null);
        setCurrentScan(toAssetSecurityScanRecord(scan));
        setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "rule") : null);
      } catch (err: any) {
        setError(`Failed to save rule: ${err}`);
      }
    }
  };

  const closeDrawer = () => {
    setSelectedId(null);
    setDisplayName("");
    setRuleContent("");
    setRuleAuthor(null);
    setIsEditing(false);
    setIsCreating(false);
    setReferencingProjects([]);
    setProjectSyncState({});
    setSyncAllState("needs-sync");
    setError(null);
    setSecurityNotice(null);
    setCurrentScan(null);
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete rule "${id}"?`, { title: "Delete Rule", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_rule", { machineName: id });
      if (selectedId === id) closeDrawer();
      await loadRules();
      setError(null);
      setSecurityNotice(null);
      setCurrentScan(null);
    } catch (err: any) {
      setError(`Failed to delete rule: ${err}`);
    }
  };

  const handleBulkDelete = async () => {
    const targets = rules.filter(r => selection.selectedIds.has(r.id) && isDeletable(r));
    if (targets.length === 0) return;

    const preview = targets.slice(0, 10).map(t => `• ${t.name}`).join("\n");
    const overflow = targets.length > 10 ? `\n…and ${targets.length - 10} more.` : "";
    const message = `Delete ${targets.length} rule${targets.length === 1 ? "" : "s"}?\n\n${preview}${overflow}\n\nThis cannot be undone.`;
    const confirmed = await ask(message, { title: "Delete Rules", kind: "warning" });
    if (!confirmed) return;

    setBulkDeleting(true);
    setBulkProgress({ done: 0, total: targets.length });
    const failed: { id: string; error: string }[] = [];
    for (let i = 0; i < targets.length; i++) {
      const id = targets[i]!.id;
      try {
        await invoke("delete_rule", { machineName: id });
      } catch (err: any) {
        failed.push({ id, error: String(err) });
      }
      setBulkProgress({ done: i + 1, total: targets.length });
    }

    if (selectedId && targets.some(t => t.id === selectedId)) {
      closeDrawer();
    }

    await loadRules();
    selection.clearSelection();
    setBulkDeleting(false);
    setBulkProgress(null);
    if (failed.length > 0) {
      const detail = failed.slice(0, 5).map(f => `${f.id}: ${f.error}`).join("\n");
      const more = failed.length > 5 ? `\n…and ${failed.length - 5} more.` : "";
      setError(`Failed to delete ${failed.length} rule${failed.length === 1 ? "" : "s"}:\n${detail}${more}`);
    } else {
      setError(null);
    }
  };

  const startCreateNew = () => {
    setSelectedId(null);
    setDisplayName("");
    setRuleContent("");
    setIsCreating(true);
    setIsEditing(true);
    setNewMachineName("");
    setNewDisplayName("");
    setReferencingProjects([]);
    setProjectSyncState({});
    setSyncAllState("needs-sync");
    setSecurityNotice(null);
    setCurrentScan(null);
    setRuleAuthor(null);
  };

  const handleSyncProject = async (projectName: string) => {
    if (!selectedId) return;
    setProjectSyncState(prev => ({ ...prev, [projectName]: "syncing" }));
    try {
      await invoke("sync_rule_to_project", { ruleName: selectedId, projectName });
      setProjectSyncState(prev => ({ ...prev, [projectName]: "synced" }));
      // Recalculate aggregate state.
      setSyncAllState(prev => {
        if (prev === "syncing") return "syncing";
        // Check if all are now synced.
        return "synced";
      });
    } catch (err: any) {
      setProjectSyncState(prev => ({ ...prev, [projectName]: "error" }));
      setError(`Failed to sync rule to project "${projectName}": ${err}`);
    }
  };

  const handleSyncAll = async () => {
    if (!selectedId || referencingProjects.length === 0) return;
    setSyncAllState("syncing");
    const initialStates: Record<string, SyncState> = {};
    for (const p of referencingProjects) initialStates[p] = "syncing";
    setProjectSyncState(initialStates);

    let hadError = false;
    for (const projectName of referencingProjects) {
      try {
        await invoke("sync_rule_to_project", { ruleName: selectedId, projectName });
        setProjectSyncState(prev => ({ ...prev, [projectName]: "synced" }));
      } catch (err: any) {
        setProjectSyncState(prev => ({ ...prev, [projectName]: "error" }));
        hadError = true;
        setError(`Failed to sync rule to project "${projectName}": ${err}`);
      }
    }
    setSyncAllState(hadError ? "error" : "synced");
  };

  const selectedEntry = rules.find(r => r.id === selectedId);
  const { label: scanStatusLabel, className: scanStatusClass } = getAssetSecurityStatus(currentScan, {
    blockedLabel: "Danger",
  });
  const scanTimestamp = currentScan
    ? new Date(currentScan.scanned_at).toLocaleString()
    : null;
  const securityNoticeToneClass = getAssetSecurityNoticeClass(currentScan);
  const securityDismissButtonClass = getAssetSecurityDismissButtonClass(currentScan);

  const handleDuplicate = async (id: string) => {
    // Strip the built-in "automatic-" prefix so the duplicate is user-owned.
    // e.g. "automatic-general" → "general-copy", never starts with "automatic-".
    const strippedId = id.startsWith("automatic-") ? id.slice("automatic-".length) : id;
    const base = `${strippedId}-copy`;
    let candidate = base;
    let suffix = 2;
    while (rules.some(r => r.id === candidate)) {
      candidate = `${base}-${suffix}`;
      suffix++;
    }
    try {
      const raw: string = await invoke("read_rule", { machineName: id });
      const rule: Rule = JSON.parse(raw);
      // Use the original display name as the starting point for the duplicate.
      const dupName = rule.name;
      await invoke("save_rule", { machineName: candidate, name: dupName, content: rule.content });
      const newEntry: RuleEntry = { id: candidate, name: dupName };
      setRules(prev => [...prev, newEntry].sort((a, b) => a.name.localeCompare(b.name)));
      await loadRule(candidate);
      setIsEditing(true);
      setError(null);
      setSecurityNotice(null);
    } catch (err: any) {
      setError(`Failed to duplicate rule: ${err}`);
    }
  };

  const searchLower = search.trim().toLowerCase();
  const filteredRules = rules.filter(r =>
    !searchLower || r.name.toLowerCase().includes(searchLower) || r.id.toLowerCase().includes(searchLower)
  );

  const selection = useBulkSelection(filteredRules, r => r.id, isDeletable);
  const drawerOpen = isCreating || !!selectedId;

  const renderTableRow = (entry: RuleEntry) => {
    const isRowSelected = selection.isSelected(entry.id);
    const isFocused = selectedId === entry.id && !isCreating;
    const deletable = isDeletable(entry);
    const origin = originLabel(entry);
    return (
      <tr
        key={entry.id}
        onClick={() => loadRule(entry.id)}
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
                isDefaultRule(entry.id)
                  ? "Default rule provided by Automatic — cannot be deleted. Duplicate to create a local copy."
                  : `Plugin-provided (${entry.plugin_id}) — cannot be deleted.`
              }
            />
          )}
        </td>
        <td className="px-3 py-2 w-11">
          <div className={ICONS.rule.iconBox}>
            <ScrollText size={15} className={ICONS.rule.iconColor} />
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
              title="Delete rule"
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
            Rules
          </span>

          <div className="flex items-center gap-2 shrink-0">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              <input
                type="text"
                placeholder="Search rules…"
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
              title="New Rule"
            >
              <Plus size={12} /> New Rule
            </button>
          </div>
        </div>

        {/* Selection action bar — appears whenever anything is selected */}
        {selection.totalSelected > 0 && (
          <div className="flex items-center justify-between px-4 py-2 border-t border-border-strong/30 bg-brand/5">
            <span className="text-[12px] text-text-base">
              {selection.totalSelected} rule{selection.totalSelected === 1 ? "" : "s"} selected
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
        items={filteredRules}
        getId={r => r.id}
        isEmpty={rules.length === 0}
        emptyState={
          <>
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-icon-rule/12 border border-icon-rule/20 flex items-center justify-center">
              <ScrollText size={22} className={ICONS.rule.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">No rules yet</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              Rules are reusable content blocks that can be appended to project instruction files. Add rules to share common guidelines across projects.
            </p>
            <button
              onClick={startCreateNew}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
            >
              <Plus size={14} /> New Rule
            </button>
          </>
        }
        noMatchState={
          <p className="text-[13px] text-text-muted">
            {searchLower ? `No rules match "${search}".` : "No rules yet."}
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
          ariaLabel: "Select all visible deletable rules",
        }}
        recentIds={recentIds}
      />

      {/* ── Drawer ───────────────────────────────────────────────────────── */}
      <AssetDrawer open={drawerOpen} onClose={closeDrawer} isEditing={isEditing} closeButtonTopClassName="top-4">
        <div className="flex-1 flex flex-col h-full min-h-0">
          {/* Header */}
          <div className="min-h-[44px] pl-6 pr-10 border-b border-border-strong/40 flex justify-between items-center gap-4 py-2 flex-shrink-0">
            <div className="flex items-center gap-3 min-w-0 flex-1">
              <FileText size={14} className={ICONS.rule.iconColor + " flex-shrink-0"} />
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
              <TokenPill text={ruleContent} />
              {selectedId && isDefaultRule(selectedId) && !isEditing && <BuiltInBadge />}
              {selectedId && isDefaultRule(selectedId) && !isEditing && (
                <ReadOnlyBadge tooltip="Default rule provided by Automatic — editing is disabled. Duplicate to create a local copy." />
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
              {!isEditing && selectedId && !isDefaultRule(selectedId) && (
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
                        if (selectedId) loadRule(selectedId);
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

          {/* Body — flex column so the projects panel is always pinned at the bottom */}
          <div className="flex-1 min-h-0 flex flex-col">
            {isEditing ? (
              <LineNumberedTextarea
                value={ruleContent}
                onChange={setRuleContent}
                className="flex-1"
                placeholder="Write your rule content here in Markdown. Rules are reusable content blocks that can be appended to project instruction files..."
              />
            ) : (
              <>
                {/* Scrollable content area */}
                <div className="flex-1 min-h-0 overflow-y-auto custom-scrollbar">
                  {/* Author section */}
                  <div className="px-6 pt-4 pb-3 border-b border-border-strong/40">
                    <AuthorSection
                      descriptor={
                        ruleAuthor
                          ? ruleAuthor
                          : selectedId && isDefaultRule(selectedId)
                          ? { type: "provider", name: "Automatic", url: "https://automatic.computer" }
                          : { type: "local" }
                      }
                    />
                  </div>
                  <div className="p-6 font-mono text-[13px] whitespace-pre-wrap text-text-base leading-relaxed">
                    {ruleContent || <span className="text-text-muted italic">This rule is empty. Click edit to add content.</span>}
                  </div>
                </div>

                {/* Used by projects panel — pinned at bottom, always visible */}
                {!isCreating && referencingProjects.length > 0 && (
                  <div className="flex-shrink-0 border-t border-border-strong/40 px-6 py-4 bg-bg-input/30">
                    <div className="flex items-center justify-between mb-3">
                      <div className="flex items-center gap-2">
                        <FolderGit2 size={13} className="text-text-muted" />
                        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                          Used in {referencingProjects.length} {referencingProjects.length === 1 ? "project" : "projects"}
                        </span>
                      </div>
                      {syncAllState === "synced" ? (
                        <span className="flex items-center gap-1.5 px-3 py-1 text-[11px] font-medium text-success">
                          In sync
                        </span>
                      ) : (
                        <button
                          onClick={handleSyncAll}
                          disabled={syncAllState === "syncing"}
                          className={`flex items-center gap-1.5 px-3 py-1 rounded text-[11px] font-medium transition-colors ${
                            syncAllState === "error"
                              ? "text-danger bg-danger/10"
                              : syncAllState === "needs-sync"
                              ? "text-warning bg-warning/10 hover:bg-warning/20"
                              : "text-text-muted hover:text-text-base hover:bg-bg-sidebar"
                          } disabled:opacity-50 disabled:cursor-not-allowed`}
                          title="Push this rule's latest content to all referencing projects"
                        >
                          <RefreshCw size={11} className={syncAllState === "syncing" ? "animate-spin" : ""} />
                          {syncAllState === "error" ? "Some failed" : "Update all"}
                        </button>
                      )}
                    </div>
                    {/* Max 3 rows visible; scrollable if more */}
                    <ul className="space-y-1.5 max-h-[108px] overflow-y-auto custom-scrollbar">
                      {referencingProjects.map(projectName => {
                        const state = projectSyncState[projectName] ?? "needs-sync";
                        return (
                          <li key={projectName} className="flex items-center justify-between gap-3 py-1">
                            <div className="flex items-center gap-2 min-w-0">
                              <div className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
                                state === "synced" ? "bg-success" : state === "error" ? "bg-danger" : "bg-warning"
                              }`} />
                              <span className="text-[13px] text-text-base truncate">{projectName}</span>
                            </div>
                            {state !== "synced" && (
                              <button
                                onClick={() => handleSyncProject(projectName)}
                                disabled={state === "syncing"}
                                className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium transition-colors flex-shrink-0 ${
                                  state === "error"
                                    ? "text-danger bg-danger/10"
                                    : state === "needs-sync"
                                    ? "text-warning bg-warning/10 hover:bg-warning/20"
                                    : "text-text-muted hover:text-text-base hover:bg-bg-sidebar"
                                } disabled:opacity-50 disabled:cursor-not-allowed`}
                                title={`Push rule to ${projectName}`}
                              >
                                <RefreshCw size={10} className={state === "syncing" ? "animate-spin" : ""} />
                                {state === "error" ? "Failed" : "Update"}
                              </button>
                            )}
                          </li>
                        );
                      })}
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
