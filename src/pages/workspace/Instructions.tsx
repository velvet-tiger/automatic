import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { LineNumberedTextarea } from "../../components/LineNumberedTextarea";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, X, Edit2, FileText, Check, ClipboardList, Search } from "lucide-react";
import { ICONS } from "../../lib/icons";
import { AuthorSection } from "../../components/AuthorPanel";
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

// Instructions are always user content — none are built-in, bundled, or
// plugin-provided, so every row is deletable.
const isDeletable = (_name: string) => true;

export default function Instructions() {
  const [instructions, setInstructions] = useState<string[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("instructions", recentRefresh);
  const [selectedInstruction, setSelectedInstruction] = useState<string | null>(null);
  const [instructionContent, setInstructionContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [newInstructionName, setNewInstructionName] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [securityNotice, setSecurityNotice] = useState<string | null>(null);
  const [currentScan, setCurrentScan] = useState<AssetSecurityScanRecord | null>(null);
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkProgress, setBulkProgress] = useState<{ done: number; total: number } | null>(null);

  useEffect(() => {
    loadInstructions();
  }, []);

  const loadInstructions = async () => {
    try {
      const result: string[] = await invoke("get_instructions");
      setInstructions(result.sort());
      setError(null);
    } catch (err: any) {
      setError(`Failed to load instructions: ${err}`);
    }
  };

  const loadInstructionContent = async (name: string) => {
    try {
      const content: string = await invoke("read_instruction", { name });
      const scan = await scanAssetContent("template", content);
      setSelectedInstruction(name);
      setInstructionContent(content);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
      setCurrentScan(toAssetSecurityScanRecord(scan));
      setSecurityNotice(
        scan.findings.length > 0
          ? formatAssetScanResult(scan, "instruction", {
              blockedHeader: "Dangerous content found in instruction:",
            })
          : null,
      );
    } catch (err: any) {
      setError(`Failed to read instruction ${name}: ${err}`);
    }
  };

  const handleSave = async () => {
    if (!selectedInstruction && !isCreating) return;
    const name = isCreating ? newInstructionName.trim() : selectedInstruction!;
    if (!name) return;
    try {
      const scan = await scanAssetContent("template", instructionContent);
      if (scan.blocked) {
        setError(formatAssetScanResult(scan, "instruction"));
        setSecurityNotice(null);
        return;
      }
      const warnings = warningFindings(scan);
      await invoke("save_instruction", { name, content: instructionContent });
      setIsEditing(false);
      setSelectedInstruction(name);
      if (isCreating) {
        setIsCreating(false);
        await loadInstructions();
        setRecentRefresh(prev => prev + 1);
      }
      setError(null);
      setCurrentScan(toAssetSecurityScanRecord(scan));
      setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "instruction") : null);
    } catch (err: any) {
      setError(`Failed to save instruction: ${err}`);
    }
  };

  const closeDrawer = () => {
    setIsCreating(false);
    setIsEditing(false);
    setSelectedInstruction(null);
    setInstructionContent("");
    setNewInstructionName("");
    setSecurityNotice(null);
    setCurrentScan(null);
  };

  const handleDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete instruction "${name}"?`, { title: "Delete Instruction", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_instruction", { name });
      if (selectedInstruction === name) closeDrawer();
      await loadInstructions();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete instruction: ${err}`);
    }
  };

  const handleBulkDelete = async () => {
    const targets = instructions.filter(name => selection.selectedIds.has(name) && isDeletable(name));
    if (targets.length === 0) return;

    const preview = targets.slice(0, 10).map(t => `• ${t}`).join("\n");
    const overflow = targets.length > 10 ? `\n…and ${targets.length - 10} more.` : "";
    const message = `Delete ${targets.length} instruction${targets.length === 1 ? "" : "s"}?\n\n${preview}${overflow}\n\nThis cannot be undone.`;
    const confirmed = await ask(message, { title: "Delete Instructions", kind: "warning" });
    if (!confirmed) return;

    setBulkDeleting(true);
    setBulkProgress({ done: 0, total: targets.length });
    const failed: { name: string; error: string }[] = [];
    for (let i = 0; i < targets.length; i++) {
      const name = targets[i]!;
      try {
        await invoke("delete_instruction", { name });
      } catch (err: any) {
        failed.push({ name, error: String(err) });
      }
      setBulkProgress({ done: i + 1, total: targets.length });
    }

    if (selectedInstruction && targets.includes(selectedInstruction)) {
      closeDrawer();
    }

    await loadInstructions();
    selection.clearSelection();
    setBulkDeleting(false);
    setBulkProgress(null);
    if (failed.length > 0) {
      const detail = failed.slice(0, 5).map(f => `${f.name}: ${f.error}`).join("\n");
      const more = failed.length > 5 ? `\n…and ${failed.length - 5} more.` : "";
      setError(`Failed to delete ${failed.length} instruction${failed.length === 1 ? "" : "s"}:\n${detail}${more}`);
    } else {
      setError(null);
    }
  };

  const startCreateNew = () => {
    setSelectedInstruction(null);
    setInstructionContent("");
    setIsCreating(true);
    setIsEditing(true);
    setNewInstructionName("");
    setSecurityNotice(null);
    setCurrentScan(null);
  };

  const { label: scanStatusLabel, className: scanStatusClass } = getAssetSecurityStatus(currentScan, {
    blockedLabel: "Danger",
  });
  const scanTimestamp = currentScan
    ? new Date(currentScan.scanned_at).toLocaleString()
    : null;
  const securityNoticeToneClass = getAssetSecurityNoticeClass(currentScan);
  const securityDismissButtonClass = getAssetSecurityDismissButtonClass(currentScan);

  const searchLower = search.trim().toLowerCase();
  const filteredInstructions = instructions.filter(
    name => !searchLower || name.toLowerCase().includes(searchLower)
  );

  const selection = useBulkSelection(filteredInstructions, name => name, isDeletable);
  const drawerOpen = isCreating || !!selectedInstruction;

  const renderTableRow = (name: string) => {
    const isRowSelected = selection.isSelected(name);
    const isFocused = selectedInstruction === name && !isCreating;
    return (
      <tr
        key={name}
        onClick={() => loadInstructionContent(name)}
        className={`group cursor-pointer border-b border-border-strong/20 last:border-b-0 transition-colors ${
          isFocused ? "bg-bg-sidebar/60" : "hover:bg-bg-input/70"
        }`}
      >
        <td className="px-3 py-2 w-9" onClick={(e) => e.stopPropagation()}>
          <input
            type="checkbox"
            checked={isRowSelected}
            onChange={() => selection.toggleSelected(name)}
            aria-label={`Select ${name}`}
            className="cursor-pointer accent-brand"
          />
        </td>
        <td className="px-3 py-2 w-11">
          <div className={ICONS.fileTemplate.iconBox}>
            <ClipboardList size={15} className={ICONS.fileTemplate.iconColor} />
          </div>
        </td>
        <td className="px-3 py-2 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-[13px] font-medium text-text-base truncate">{name}</span>
            {recentIds.has(name) && (
              <span className="shrink-0 px-1.5 py-0.5 rounded bg-brand/15 text-brand text-[9px] font-semibold uppercase tracking-wider">New</span>
            )}
          </div>
        </td>
        <td className="px-3 py-2 w-16 text-right" onClick={(e) => e.stopPropagation()}>
          <button
            onClick={(e) => handleDelete(name, e)}
            className="opacity-0 group-hover:opacity-100 p-1 text-text-muted hover:text-danger rounded transition-all"
            title="Delete instruction"
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
            Instructions
          </span>

          <div className="flex items-center gap-2 shrink-0">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              <input
                type="text"
                placeholder="Search instructions…"
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
              title="New Instruction"
            >
              <Plus size={12} /> New Instruction
            </button>
          </div>
        </div>

        {/* Selection action bar — appears whenever anything is selected */}
        {selection.totalSelected > 0 && (
          <div className="flex items-center justify-between px-4 py-2 border-t border-border-strong/30 bg-brand/5">
            <span className="text-[12px] text-text-base">
              {selection.totalSelected} instruction{selection.totalSelected === 1 ? "" : "s"} selected
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
        items={filteredInstructions}
        getId={name => name}
        isEmpty={instructions.length === 0}
        emptyState={
          <>
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-icon-file-template/12 border border-icon-file-template/20 flex items-center justify-center">
              <ClipboardList size={22} className={ICONS.fileTemplate.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">No instructions yet</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              Instructions are reusable project files, providing the base context for all actions taken in your project.
            </p>
            <button
              onClick={startCreateNew}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
            >
              <Plus size={14} /> New Instruction
            </button>
          </>
        }
        noMatchState={
          <p className="text-[13px] text-text-muted">
            {searchLower ? `No instructions match "${search}".` : "No instructions yet."}
          </p>
        }
        columns={[
          { key: "icon", header: "", className: "w-11" },
          { key: "name", header: "Name" },
          { key: "actions", header: "", className: "w-16" },
        ]}
        renderRow={renderTableRow}
        selection={{
          allSelected: selection.allSelected,
          someSelected: selection.someSelected,
          disabled: selection.deletableItems.length === 0,
          onToggleAll: selection.toggleSelectAllVisible,
          ariaLabel: "Select all visible instructions",
        }}
        recentIds={recentIds}
      />

      {/* ── Drawer ───────────────────────────────────────────────────────── */}
      <AssetDrawer open={drawerOpen} onClose={closeDrawer} isEditing={isEditing}>
        <div className="flex-1 flex flex-col h-full">
          {/* Header */}
          <div className="h-11 pl-6 pr-10 border-b border-border-strong/40 flex justify-between items-center shrink-0">
            <div className="flex items-center gap-3 min-w-0">
              <FileText size={14} className={`${ICONS.fileTemplate.iconColor} shrink-0`} />
              {isCreating ? (
                <input
                  type="text"
                  placeholder="instruction-name (no spaces/slashes)"
                  value={newInstructionName}
                  onChange={(e) => setNewInstructionName(e.target.value)}
                  autoFocus
                  className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-64"
                />
              ) : (
                <h3 className="text-[14px] font-medium text-text-base truncate">{selectedInstruction}</h3>
              )}
            </div>

            <div className="flex items-center gap-2 shrink-0">
              <TokenPill text={instructionContent} />
              {!isEditing ? (
                <button
                  onClick={() => setIsEditing(true)}
                  className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                >
                  <Edit2 size={12} /> Edit
                </button>
              ) : (
                <>
                  {!isCreating && (
                    <button
                      onClick={() => {
                        setIsEditing(false);
                        loadInstructionContent(selectedInstruction!);
                      }}
                      className="px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                    >
                      Cancel
                    </button>
                  )}
                  <button
                    onClick={handleSave}
                    disabled={isCreating && !newInstructionName.trim()}
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
          <div className="flex-1 flex flex-col relative min-h-0">
            {isEditing ? (
              <LineNumberedTextarea
                value={instructionContent}
                onChange={setInstructionContent}
                className="flex-1"
                placeholder="Write your instruction content here in Markdown..."
              />
            ) : (
              <>
                {/* Author section */}
                <div className="px-6 pt-4 pb-3 border-b border-border-strong/40 shrink-0">
                  <AuthorSection descriptor={{ type: "local" }} />
                </div>
                <div className="flex-1 overflow-y-auto p-6 font-mono text-[13px] whitespace-pre-wrap text-text-base leading-relaxed custom-scrollbar">
                  {instructionContent || <span className="text-text-muted italic">This instruction is empty. Click edit to add content.</span>}
                </div>
              </>
            )}
          </div>
        </div>
      </AssetDrawer>
    </div>
  );
}
