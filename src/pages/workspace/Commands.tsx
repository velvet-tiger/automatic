import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { escapeYamlDoubleQuoted } from "../../lib/yaml";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { LineNumberedTextarea } from "../../components/LineNumberedTextarea";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, Terminal, Check, Edit2, X, Search } from "lucide-react";
import { ICONS } from "../../lib/icons";
import { TokenPill } from "../../components/TokenPill";
import { AssetTable } from "../../components/AssetTable";
import { AssetDrawer } from "../../components/AssetDrawer";
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

interface UserCommandEntry {
  id: string;
  description: string;
}

// Commands are always user content — none are built-in, bundled, or
// plugin-provided, so every row is deletable.
const isDeletable = (_entry: UserCommandEntry) => true;

/** Parse command markdown into frontmatter description + body. */
function parseCommandContent(raw: string): { description: string; body: string } {
  const match = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!match) return { description: "", body: raw };

  let description = "";
  for (const line of match[1]!.split("\n")) {
    const trimmed = line.trim();
    const value = trimmed.match(/^description:\s*(.*)$/)?.[1];
    if (value !== undefined) {
      description = value.replace(/^["']|["']$/g, "");
    }
  }

  return { description, body: match[2]!.trimStart() };
}

/** Rebuild full markdown from description + body. */
function buildCommandContent(description: string, body: string): string {
  const safeDesc = description.includes(":") ? `"${escapeYamlDoubleQuoted(description)}"` : description;
  return `---\ndescription: ${safeDesc}\n---\n\n${body}`;
}

function validateCommandDescription(value: string): string | null {
  if (!value.trim()) return "Description is required.";
  if (value.length > 256) return "Description must be 256 characters or fewer.";
  return null;
}

const DEFAULT_COMMAND_BODY = `Write the reusable prompt here.`;

/** Coerce raw input into a valid command name: lowercase, digits, hyphens. */
function toCommandName(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, "-")
    .replace(/-{2,}/g, "-");
}

interface CommandsProps {
  /** Pre-select this command when the component mounts / when it changes. */
  initialCommand?: string | null;
  /** Called once the initial command has been applied so the parent can clear it. */
  onInitialCommandConsumed?: () => void;
}

export default function Commands({ initialCommand = null, onInitialCommandConsumed }: CommandsProps = {}) {
  const [commands, setCommands] = useState<UserCommandEntry[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("commands", recentRefresh);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editDescription, setEditDescription] = useState("");
  const [editBody, setEditBody] = useState("");
  const [descriptionError, setDescriptionError] = useState<string | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newMachineName, setNewMachineName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [securityNotice, setSecurityNotice] = useState<string | null>(null);
  const [currentScan, setCurrentScan] = useState<AssetSecurityScanRecord | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");
  const [search, setSearch] = useState("");
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkProgress, setBulkProgress] = useState<{ done: number; total: number } | null>(null);

  useEffect(() => {
    void loadCommands();
  }, []);

  // Navigate to the command specified by the parent (e.g. "View in library" from Projects)
  useEffect(() => {
    if (!initialCommand) return;
    if (commands.length === 0) return;
    const exists = commands.some((c) => c.id === initialCommand);
    if (exists) {
      void loadCommand(initialCommand);
    }
    onInitialCommandConsumed?.();
  }, [initialCommand, commands]);

  const loadCommands = async () => {
    try {
      const result: UserCommandEntry[] = await invoke("get_user_commands");
      setCommands(result.sort((a, b) => a.id.localeCompare(b.id)));
      setError(null);
    } catch (err: any) {
      setError(`Failed to load commands: ${err}`);
    }
  };

  const loadCommand = async (id: string) => {
    try {
      const raw: string = await invoke("read_user_command", { machineName: id });
      const scan = await scanAssetContent("user_command", raw);
      const { description, body } = parseCommandContent(raw);
      setSelectedId(id);
      setEditDescription(description);
      setEditBody(body);
      setDescriptionError(null);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
      setCurrentScan(toAssetSecurityScanRecord(scan));
      setSecurityNotice(
        scan.findings.length > 0
          ? formatAssetScanResult(scan, "command", {
              blockedHeader: "Dangerous content found in command:",
            })
          : null,
      );
    } catch (err: any) {
      setError(`Failed to read command: ${err}`);
    }
  };

  const handleSave = async () => {
    const id = isCreating ? newMachineName.trim() : selectedId;
    if (!id) return;

    const descErr = validateCommandDescription(editDescription);
    if (descErr) {
      setDescriptionError(descErr);
      return;
    }

    try {
      const content = buildCommandContent(editDescription.trim(), editBody);
      const scan = await scanAssetContent("user_command", content);
      if (scan.blocked) {
        setError(formatAssetScanResult(scan, "command"));
        setSecurityNotice(null);
        return;
      }
      const warnings = warningFindings(scan);
      const isNew = isCreating;
      await invoke("save_user_command", { machineName: id, content });
      await loadCommands();
      setSelectedId(id);
      setIsCreating(false);
      setIsEditing(false);
      setDescriptionError(null);
      setError(null);
      setCurrentScan(toAssetSecurityScanRecord(scan));
      setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "command") : null);
      if (isNew) {
        setRecentRefresh(prev => prev + 1);
      }
    } catch (err: any) {
      setError(`Failed to save command: ${err}`);
    }
  };

  const closeDrawer = () => {
    setSelectedId(null);
    setEditDescription("");
    setEditBody("");
    setDescriptionError(null);
    setIsEditing(false);
    setIsCreating(false);
    setIsRenaming(false);
    setError(null);
    setSecurityNotice(null);
    setCurrentScan(null);
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete command "${id}"?`, { title: "Delete Command", kind: "warning" });
    if (!confirmed) return;

    try {
      await invoke("delete_user_command", { machineName: id });
      if (selectedId === id) closeDrawer();
      await loadCommands();
      setError(null);
      setSecurityNotice(null);
      setCurrentScan(null);
    } catch (err: any) {
      setError(`Failed to delete command: ${err}`);
    }
  };

  const handleBulkDelete = async () => {
    const targets = commands.filter(c => selection.selectedIds.has(c.id) && isDeletable(c));
    if (targets.length === 0) return;

    const preview = targets.slice(0, 10).map(t => `• /${t.id}`).join("\n");
    const overflow = targets.length > 10 ? `\n…and ${targets.length - 10} more.` : "";
    const message = `Delete ${targets.length} command${targets.length === 1 ? "" : "s"}?\n\n${preview}${overflow}\n\nThis cannot be undone.`;
    const confirmed = await ask(message, { title: "Delete Commands", kind: "warning" });
    if (!confirmed) return;

    setBulkDeleting(true);
    setBulkProgress({ done: 0, total: targets.length });
    const failed: { id: string; error: string }[] = [];
    for (let i = 0; i < targets.length; i++) {
      const id = targets[i]!.id;
      try {
        await invoke("delete_user_command", { machineName: id });
      } catch (err: any) {
        failed.push({ id, error: String(err) });
      }
      setBulkProgress({ done: i + 1, total: targets.length });
    }

    if (selectedId && targets.some(t => t.id === selectedId)) {
      closeDrawer();
    }

    await loadCommands();
    selection.clearSelection();
    setBulkDeleting(false);
    setBulkProgress(null);
    if (failed.length > 0) {
      const detail = failed.slice(0, 5).map(f => `/${f.id}: ${f.error}`).join("\n");
      const more = failed.length > 5 ? `\n…and ${failed.length - 5} more.` : "";
      setError(`Failed to delete ${failed.length} command${failed.length === 1 ? "" : "s"}:\n${detail}${more}`);
    } else {
      setError(null);
    }
  };

  const startCreateNew = () => {
    setSelectedId(null);
    setEditDescription("");
    setEditBody(DEFAULT_COMMAND_BODY);
    setDescriptionError(null);
    setIsCreating(true);
    setIsEditing(true);
    setNewMachineName("");
    setError(null);
    setSecurityNotice(null);
    setCurrentScan(null);
  };

  const startRename = () => {
    if (!selectedId || isCreating) return;
    setRenameName(selectedId);
    setIsRenaming(true);
  };

  const handleRename = async () => {
    const trimmed = renameName.trim();
    if (!selectedId || !trimmed || trimmed === selectedId) {
      setIsRenaming(false);
      return;
    }
    try {
      await invoke("rename_user_command", { oldName: selectedId, newName: trimmed });
      await loadCommands();
      setSelectedId(trimmed);
      setIsRenaming(false);
      setError(null);
      setSecurityNotice(null);
    } catch (err: any) {
      setError(`Failed to rename command: ${err}`);
    }
  };

  const selectedEntry = commands.find((entry) => entry.id === selectedId) ?? null;
  const { label: scanStatusLabel, className: scanStatusClass } = getAssetSecurityStatus(currentScan, {
    blockedLabel: "Danger",
  });
  const scanTimestamp = currentScan
    ? new Date(currentScan.scanned_at).toLocaleString()
    : null;
  const securityNoticeToneClass = getAssetSecurityNoticeClass(currentScan);
  const securityDismissButtonClass = getAssetSecurityDismissButtonClass(currentScan);

  const searchLower = search.trim().toLowerCase();
  const filteredCommands = commands.filter(c =>
    !searchLower || c.id.toLowerCase().includes(searchLower) || c.description.toLowerCase().includes(searchLower)
  );

  const selection = useBulkSelection(filteredCommands, c => c.id, isDeletable);
  const drawerOpen = isCreating || !!selectedEntry;

  const renderTableRow = (entry: UserCommandEntry) => {
    const isRowSelected = selection.isSelected(entry.id);
    const isFocused = selectedId === entry.id && !isCreating;
    return (
      <tr
        key={entry.id}
        onClick={() => void loadCommand(entry.id)}
        className={`group cursor-pointer border-b border-border-strong/20 last:border-b-0 transition-colors ${
          isFocused ? "bg-bg-sidebar/60" : "hover:bg-bg-input/70"
        }`}
      >
        <td className="px-3 py-2 w-9" onClick={(e) => e.stopPropagation()}>
          <input
            type="checkbox"
            checked={isRowSelected}
            onChange={() => selection.toggleSelected(entry.id)}
            aria-label={`Select ${entry.id}`}
            className="cursor-pointer accent-brand"
          />
        </td>
        <td className="px-3 py-2 w-11">
          <div className={ICONS.command.iconBox}>
            <Terminal size={15} className={ICONS.command.iconColor} />
          </div>
        </td>
        <td className="px-3 py-2 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-[13px] font-medium text-text-base truncate">/{entry.id}</span>
            {recentIds.has(entry.id) && (
              <span className="shrink-0 px-1.5 py-0.5 rounded bg-brand/15 text-brand text-[9px] font-semibold uppercase tracking-wider">New</span>
            )}
          </div>
        </td>
        <td className="px-3 py-2 min-w-0">
          <span className="block text-[12px] text-text-muted truncate max-w-[360px]">
            {entry.description || <span className="italic text-text-muted/60">No description</span>}
          </span>
        </td>
        <td className="px-3 py-2 w-16 text-right" onClick={(e) => e.stopPropagation()}>
          <button
            onClick={(e) => void handleDelete(entry.id, e)}
            className="opacity-0 group-hover:opacity-100 p-1 text-text-muted hover:text-danger rounded transition-all"
            title="Delete command"
          >
            <X size={13} />
          </button>
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
            Commands
          </span>

          <div className="flex items-center gap-2 shrink-0">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              <input
                type="text"
                placeholder="Search commands…"
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
              title="New Command"
            >
              <Plus size={12} /> New Command
            </button>
          </div>
        </div>

        {/* Selection action bar — appears whenever anything is selected */}
        {selection.totalSelected > 0 && (
          <div className="flex items-center justify-between px-4 py-2 border-t border-border-strong/30 bg-brand/5">
            <span className="text-[12px] text-text-base">
              {selection.totalSelected} command{selection.totalSelected === 1 ? "" : "s"} selected
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
          <button onClick={() => setError(null)} className="text-red-900/70 hover:text-red-950 transition-colors">
            <X size={14} />
          </button>
        </div>
      )}
      {securityNotice && (
        <div className={`${securityNoticeToneClass} p-3 text-[13px] border-b flex items-center justify-between shrink-0`}>
          <div className="whitespace-pre-wrap">{securityNotice}</div>
          <button onClick={() => setSecurityNotice(null)} className={securityDismissButtonClass}>
            <X size={14} />
          </button>
        </div>
      )}

      {/* ── Table ────────────────────────────────────────────────────────── */}
      <AssetTable
        items={filteredCommands}
        getId={c => c.id}
        isEmpty={commands.length === 0}
        emptyState={
          <>
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-icon-agent/12 border border-icon-agent/20 flex items-center justify-center">
              <Terminal size={22} className={ICONS.command.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">No commands yet</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              Commands are reusable prompts that agents can trigger by name. Create your first command to build a library of common workflows.
            </p>
            <button
              onClick={startCreateNew}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
            >
              <Plus size={14} /> New Command
            </button>
          </>
        }
        noMatchState={
          <p className="text-[13px] text-text-muted">
            {searchLower ? `No commands match "${search}".` : "No commands yet."}
          </p>
        }
        columns={[
          { key: "icon", header: "", className: "w-11" },
          { key: "name", header: "Command" },
          { key: "description", header: "Description" },
          { key: "actions", header: "", className: "w-16" },
        ]}
        renderRow={renderTableRow}
        selection={{
          allSelected: selection.allSelected,
          someSelected: selection.someSelected,
          disabled: selection.deletableItems.length === 0,
          onToggleAll: selection.toggleSelectAllVisible,
          ariaLabel: "Select all visible commands",
        }}
        recentIds={recentIds}
      />

      {/* ── Drawer ───────────────────────────────────────────────────────── */}
      <AssetDrawer open={drawerOpen} onClose={closeDrawer} isEditing={isEditing} closeButtonTopClassName="top-4">
        <div className="flex-1 flex flex-col h-full min-h-0">
          <div className="min-h-[44px] pl-5 pr-10 py-2 border-b border-border-strong/40 flex items-center justify-between shrink-0">
            <div className="flex items-center gap-3 min-w-0">
              <div className={ICONS.command.iconBox}>
                <Terminal size={15} className={ICONS.command.iconColor} />
              </div>
              <div className="min-w-0">
                {isCreating ? (
                  <input
                    type="text"
                    value={newMachineName}
                    onChange={(e) => setNewMachineName(toCommandName(e.target.value))}
                    placeholder="command-name"
                    autoFocus
                    className="bg-transparent outline-none text-[15px] font-semibold text-text-base placeholder-text-muted/50"
                  />
                ) : isRenaming ? (
                  <input
                    type="text"
                    value={renameName}
                    onChange={(e) => setRenameName(toCommandName(e.target.value))}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleRename();
                      if (e.key === "Escape") setIsRenaming(false);
                    }}
                    onBlur={() => void handleRename()}
                    autoFocus
                    className="bg-transparent outline-none text-[15px] font-semibold text-text-base placeholder-text-muted/50"
                  />
                ) : (
                  <div
                    className="text-[15px] font-semibold text-text-base truncate cursor-text"
                    onDoubleClick={startRename}
                    title="Double-click to rename"
                  >
                    /{selectedEntry?.id}
                  </div>
                )}
                <div className="text-[11px] text-text-muted">Workspace command library</div>
              </div>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              {!isEditing ? (
                <button
                  onClick={() => setIsEditing(true)}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-border-strong/50 text-[12px] text-text-base hover:bg-bg-sidebar transition-colors"
                >
                  <Edit2 size={12} /> Edit
                </button>
              ) : (
                <>
                  {!isCreating && (
                    <button
                      onClick={() => {
                        setIsEditing(false);
                        setDescriptionError(null);
                        if (selectedId) void loadCommand(selectedId);
                      }}
                      className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-border-strong/50 text-[12px] text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors"
                    >
                      <X size={12} /> Cancel
                    </button>
                  )}
                  <button
                    onClick={() => void handleSave()}
                    className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-brand text-white text-[12px] hover:bg-brand-hover transition-colors"
                  >
                    <Check size={12} /> Save
                  </button>
                </>
              )}
            </div>
          </div>

          {!isEditing && (
            <div className="px-5 py-2.5 border-b border-border-strong/40 flex items-center gap-2 shrink-0 bg-bg-input/20">
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

          <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
            {isEditing ? (
              <>
                {/* Description field */}
                <div className="px-6 pt-5 pb-4 border-b border-border-strong/40 shrink-0 space-y-4">
                  <div>
                    <div className="flex items-baseline justify-between mb-1.5">
                      <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                        Description <span className="text-red-400 ml-0.5">*</span>
                      </label>
                      <span className={`text-[11px] tabular-nums ${editDescription.length > 220 ? (editDescription.length > 256 ? "text-red-400" : "text-warning") : "text-text-muted"}`}>
                        {editDescription.length}/256
                      </span>
                    </div>
                    <textarea
                      placeholder="A concise description of what this command does."
                      value={editDescription}
                      onChange={(e) => {
                        setEditDescription(e.target.value);
                        setDescriptionError(validateCommandDescription(e.target.value));
                      }}
                      rows={2}
                      maxLength={256}
                      className={`w-full px-3 py-2 rounded-md bg-bg-sidebar border outline-none text-[13px] text-text-base placeholder-text-muted/40 resize-none transition-colors leading-relaxed ${
                        descriptionError ? "border-red-500/60 focus:border-red-500" : "border-border-strong/40 hover:border-border-strong focus:border-brand"
                      }`}
                      spellCheck={false}
                    />
                    {descriptionError && (
                      <p className="mt-1 text-[11px] text-red-400">{descriptionError}</p>
                    )}
                  </div>
                </div>

                {/* Body textarea */}
                <div className="flex-1 min-h-0 flex flex-col">
                  <div className="px-6 pt-3 pb-2 shrink-0 flex items-center justify-between">
                    <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                      Prompt
                    </label>
                    <TokenPill text={buildCommandContent(editDescription, editBody)} />
                  </div>
                  <LineNumberedTextarea
                    value={editBody}
                    onChange={setEditBody}
                    className="flex-1"
                  />
                </div>
              </>
            ) : (
              <div className="flex-1 overflow-y-auto custom-scrollbar p-5 space-y-4">
                <div className="flex items-center gap-3 text-[12px] text-text-muted">
                  <TokenPill text={buildCommandContent(editDescription, editBody)} />
                  <span>Stored as Markdown and synced into provider-specific command formats.</span>
                </div>

                {/* Read-only description */}
                <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-3">
                  <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1.5">Description</div>
                  <div className="text-[13px] text-text-base leading-relaxed">
                    {editDescription || <span className="text-text-muted italic">No description</span>}
                  </div>
                </div>

                {/* Read-only body */}
                <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-3">
                  <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1.5">Prompt</div>
                  <pre className="text-[12px] font-mono text-text-base leading-relaxed whitespace-pre-wrap">
                    {editBody || <span className="text-text-muted italic">No content</span>}
                  </pre>
                </div>
              </div>
            )}
          </div>
        </div>
      </AssetDrawer>
    </div>
  );
}
