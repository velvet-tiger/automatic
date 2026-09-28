import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { LineNumberedTextarea } from "../../components/LineNumberedTextarea";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, X, Edit2, Check, MessagesSquare, Copy, FolderGit2, Search, ChevronDown, ChevronRight } from "lucide-react";
import { AuthorSection } from "../../components/AuthorPanel";
import { TokenPill } from "../../components/TokenPill";
import { AssetTable } from "../../components/AssetTable";
import { AssetDrawer } from "../../components/AssetDrawer";
import { BuiltInBadge, ReadOnlyBadge, LockCell } from "../../components/ProtectionBadge";
import { useBulkSelection } from "../../lib/useBulkSelection";
import { nextAvailableName } from "../../lib/uniqueName";
import { validateLibraryName } from "../../lib/libraryNames";
import { MarkdownPreview } from "../../components/MarkdownPreview";
import {
  type SubagentDocument,
  type SubagentFields,
  conflictingExtraKeys,
  duplicateSubagentIdentity,
  parseSubagent,
  serializeSubagent,
  subagentMachineName,
  withSubagentName,
} from "../../lib/subagentDocument";
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
  /** The project's `local_key`, when it is registered and has one. */
  local_key?: string;
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

/** A new agent starts from the default template with the identity left blank. */
function newAgentDraft(): SubagentDocument {
  const doc = parseSubagent(DEFAULT_AGENT_CONTENT);
  return { ...doc, fields: { ...doc.fields, name: "", description: "" } };
}

/** Surrounding whitespace is never meaningful in a front matter field. */
function trimmedDraft(doc: SubagentDocument): SubagentDocument {
  const fields = { ...doc.fields };
  for (const key of Object.keys(fields) as (keyof SubagentFields)[]) {
    fields[key] = fields[key].trim();
  }
  return { ...doc, fields };
}

const MODEL_OPTIONS: { value: string; label: string }[] = [
  { value: "", label: "Default" },
  { value: "inherit", label: "Same as main conversation" },
  { value: "sonnet", label: "Sonnet" },
  { value: "opus", label: "Opus" },
  { value: "haiku", label: "Haiku" },
];
const CUSTOM_MODEL = "__custom__";

const COLOR_OPTIONS = ["red", "blue", "green", "yellow", "purple", "orange", "pink", "cyan"];

const FIELD_LABEL_CLASS = "block mb-1.5 text-[11px] font-semibold text-text-muted tracking-wider uppercase";
// Border colour is left out so a field can swap it (for example on error)
// without two border-colour utilities competing.
const FIELD_INPUT_BASE_CLASS =
  "w-full text-[12px] text-text-base bg-bg-input border rounded-md px-2.5 py-1 placeholder-text-muted/50 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors";
const FIELD_INPUT_CLASS = `${FIELD_INPUT_BASE_CLASS} border-border-strong/50`;
const FIELD_VALUE_CLASS = "text-[13px] text-text-base leading-relaxed break-words";
const FIELD_EMPTY_CLASS = "text-[13px] text-text-muted";
const FIELD_SELECT_CLASS =
  "w-full appearance-none text-[12px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 pr-7 py-1 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors";

interface SubagentFieldsEditorProps {
  draft: SubagentDocument;
  onChange: (next: SubagentDocument) => void;
}

/**
 * Form for a sub-agent's front matter and prompt. Known keys get their own
 * fields. Any other front matter is edited as raw YAML so nothing is lost.
 * It mounts fresh each time editing starts, which resets its local UI state.
 */
function SubagentFieldsEditor({ draft, onChange }: SubagentFieldsEditorProps) {
  const { fields } = draft;
  const modelIsCustom = fields.model !== "" && !MODEL_OPTIONS.some(o => o.value === fields.model);
  const [customModel, setCustomModel] = useState(modelIsCustom);
  const [showExtra, setShowExtra] = useState(draft.extraFrontmatter.trim() !== "");
  const conflicts = conflictingExtraKeys(draft);
  const showModelId = customModel || modelIsCustom;
  // An empty name is reported by the disabled Save button, not as an error.
  const nameError = fields.name.trim() === "" ? null : validateLibraryName(fields.name.trim());

  const setField = (key: keyof SubagentFields, value: string) =>
    onChange({ ...draft, fields: { ...fields, [key]: value } });

  const colorOptions = fields.color !== "" && !COLOR_OPTIONS.includes(fields.color)
    ? [...COLOR_OPTIONS, fields.color]
    : COLOR_OPTIONS;

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="px-6 pt-5 pb-4 border-b border-border-strong/40 shrink-0 space-y-4 max-h-[55%] overflow-y-auto custom-scrollbar">
        <div>
          <label htmlFor="subagent-name" className={FIELD_LABEL_CLASS}>
            Name <span className="text-danger ml-0.5">*</span>
          </label>
          <input
            id="subagent-name"
            type="text"
            value={fields.name}
            onChange={(e) => setField("name", e.target.value)}
            placeholder="code-reviewer"
            autoFocus
            spellCheck={false}
            aria-invalid={nameError !== null}
            className={`${FIELD_INPUT_BASE_CLASS} font-mono ${nameError ? "border-danger/60" : "border-border-strong/50"}`}
          />
          {nameError ? (
            <p className="mt-1 text-[11px] text-danger">{nameError}</p>
          ) : (
            <p className="mt-1 text-[11px] text-text-muted">
              Agents call the sub-agent by this name. Use lowercase letters, numbers, and hyphens.
            </p>
          )}
        </div>

        <div>
          <label htmlFor="subagent-description" className={FIELD_LABEL_CLASS}>
            Description <span className="text-danger ml-0.5">*</span>
          </label>
          <textarea
            id="subagent-description"
            value={fields.description}
            onChange={(e) => setField("description", e.target.value)}
            placeholder="What this agent does and when to hand work to it."
            rows={2}
            className={`${FIELD_INPUT_CLASS} resize-none leading-relaxed`}
          />
        </div>

        <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,11rem)_minmax(0,8rem)] gap-3">
          <div className="min-w-0">
            <label htmlFor="subagent-tools" className={FIELD_LABEL_CLASS}>Tools</label>
            <input
              id="subagent-tools"
              type="text"
              value={fields.tools}
              onChange={(e) => setField("tools", e.target.value)}
              placeholder="All tools"
              spellCheck={false}
              className={FIELD_INPUT_CLASS}
            />
          </div>

          <div className="min-w-0">
            <label htmlFor="subagent-model" className={FIELD_LABEL_CLASS}>Model</label>
            <div className="relative">
              <select
                id="subagent-model"
                value={showModelId ? CUSTOM_MODEL : fields.model}
                onChange={(e) => {
                  if (e.target.value === CUSTOM_MODEL) {
                    setCustomModel(true);
                    return;
                  }
                  setCustomModel(false);
                  setField("model", e.target.value);
                }}
                className={FIELD_SELECT_CLASS}
              >
                {MODEL_OPTIONS.map(o => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
                <option value={CUSTOM_MODEL}>Other model ID…</option>
              </select>
              <ChevronDown size={12} className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted" />
            </div>
          </div>

          <div className="min-w-0">
            <label htmlFor="subagent-color" className={FIELD_LABEL_CLASS}>Colour</label>
            <div className="relative">
              <select
                id="subagent-color"
                value={fields.color}
                onChange={(e) => setField("color", e.target.value)}
                className={`${FIELD_SELECT_CLASS} capitalize`}
              >
                <option value="">Default</option>
                {colorOptions.map(c => (
                  <option key={c} value={c}>{c}</option>
                ))}
              </select>
              <ChevronDown size={12} className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted" />
            </div>
          </div>
        </div>
        <p className="-mt-2 text-[11px] text-text-muted">
          Separate tools with commas, for example <code className="font-mono">Read, Grep, Bash</code>. Leave empty to allow every tool.
        </p>

        {showModelId && (
          <div>
            <label htmlFor="subagent-model-id" className={FIELD_LABEL_CLASS}>Model ID</label>
            <input
              id="subagent-model-id"
              type="text"
              value={fields.model}
              onChange={(e) => setField("model", e.target.value)}
              placeholder="claude-sonnet-4-5"
              spellCheck={false}
              className={`${FIELD_INPUT_CLASS} font-mono`}
            />
          </div>
        )}

        <div>
          <button
            type="button"
            onClick={() => setShowExtra(prev => !prev)}
            className="flex items-center gap-1 text-[11px] font-semibold text-text-muted tracking-wider uppercase hover:text-text-base transition-colors"
            aria-expanded={showExtra}
          >
            {showExtra ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            Other settings
          </button>
          {showExtra && (
            <div className="mt-1.5">
              <textarea
                value={draft.extraFrontmatter}
                onChange={(e) => onChange({ ...draft, extraFrontmatter: e.target.value })}
                placeholder="permissionMode: plan"
                rows={4}
                spellCheck={false}
                aria-label="Other front matter settings as YAML"
                className={`${FIELD_INPUT_CLASS} font-mono resize-y leading-relaxed`}
              />
              <p className="mt-1 text-[11px] text-text-muted">
                Extra front matter in YAML, such as hooks or permissionMode. Saved exactly as written.
              </p>
            </div>
          )}
          {conflicts.length > 0 && (
            <p className="mt-1 text-[11px] text-danger">
              Remove {conflicts.join(", ")} from other settings. {conflicts.length === 1 ? "It is" : "They are"} already set above.
            </p>
          )}
        </div>
      </div>

      <div className="flex-1 min-h-0 flex flex-col">
        <div className="px-6 pt-3 pb-2 shrink-0">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Prompt</span>
        </div>
        <LineNumberedTextarea
          value={draft.body}
          onChange={(body) => onChange({ ...draft, body })}
          className="flex-1"
          placeholder="Write the instructions this agent follows."
        />
      </div>
    </div>
  );
}

function modelLabel(model: string): string {
  return MODEL_OPTIONS.find(o => o.value === model)?.label ?? model;
}

/**
 * Read-only view of a sub-agent. It uses the editor's labels and layout so
 * viewing and editing look the same, and never shows the raw file.
 */
function SubagentFieldsView({ doc }: { doc: SubagentDocument }) {
  const { fields } = doc;
  return (
    <>
      <div className="px-6 pt-5 pb-4 border-b border-border-strong/40 space-y-4">
        <div>
          <span className={FIELD_LABEL_CLASS}>Name</span>
          {fields.name
            ? <div className={`${FIELD_VALUE_CLASS} font-mono`}>{fields.name}</div>
            : <div className={FIELD_EMPTY_CLASS}>Not set</div>}
        </div>

        <div>
          <span className={FIELD_LABEL_CLASS}>Description</span>
          {fields.description
            ? <div className={`${FIELD_VALUE_CLASS} whitespace-pre-wrap`}>{fields.description}</div>
            : <div className={FIELD_EMPTY_CLASS}>Not set</div>}
        </div>

        <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,11rem)_minmax(0,8rem)] gap-3">
          <div className="min-w-0">
            <span className={FIELD_LABEL_CLASS}>Tools</span>
            {fields.tools
              ? <div className={FIELD_VALUE_CLASS}>{fields.tools}</div>
              : <div className={FIELD_EMPTY_CLASS}>All tools</div>}
          </div>
          <div className="min-w-0">
            <span className={FIELD_LABEL_CLASS}>Model</span>
            <div className={FIELD_VALUE_CLASS}>{modelLabel(fields.model)}</div>
          </div>
          <div className="min-w-0">
            <span className={FIELD_LABEL_CLASS}>Colour</span>
            {fields.color
              ? <div className={`${FIELD_VALUE_CLASS} capitalize`}>{fields.color}</div>
              : <div className={FIELD_EMPTY_CLASS}>Default</div>}
          </div>
        </div>

        {doc.extraFrontmatter.trim() !== "" && (
          <div>
            <span className={FIELD_LABEL_CLASS}>Other settings</span>
            <pre className="bg-bg-input border border-border-strong/40 rounded-md px-3 py-2 font-mono text-[11px] text-text-base leading-relaxed overflow-x-auto whitespace-pre">
              {doc.extraFrontmatter}
            </pre>
          </div>
        )}
      </div>

      <div className="px-6 pt-3 pb-6">
        <span className={FIELD_LABEL_CLASS}>Prompt</span>
        {doc.body.trim()
          ? <MarkdownPreview content={doc.body} />
          : <div className={`${FIELD_EMPTY_CLASS} italic`}>This agent has no prompt. Click Edit to add one.</div>}
      </div>
    </>
  );
}

export default function Subagents() {
  const [agents, setAgents] = useState<SubagentEntry[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("user_agents", recentRefresh);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [displayName, setDisplayName] = useState("");
  const [agentContent, setAgentContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  // The editable form of the agent. Set whenever an agent is loaded or created.
  const [draft, setDraft] = useState<SubagentDocument | null>(null);
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
      setDraft(parseSubagent(agent.content));
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
    if (!draft) return;
    const finalDraft = trimmedDraft(draft);
    const name = finalDraft.fields.name;
    if (
      validateLibraryName(name) !== null
      || !finalDraft.fields.description
      || conflictingExtraKeys(finalDraft).length > 0
    ) return;
    // A new agent's file name comes from its name, so the user never picks a slug.
    const id = isCreating
      ? nextAvailableName(subagentMachineName(name), agents.map(a => a.id))
      : selectedId;
    if (!id) return;
    const content = serializeSubagent(finalDraft);
    try {
      const scan = await scanAssetContent("user_agent", content);
      if (scan.blocked) {
        setError(formatAssetScanResult(scan, "user agent"));
        setSecurityNotice(null);
        return;
      }
      const warnings = warningFindings(scan);
      await invoke("save_subagent", { machineName: id, name, content });
      if (isCreating) {
        const newEntry: SubagentEntry = { id, name };
        setAgents(prev => [...prev, newEntry].sort((a, b) => a.name.localeCompare(b.name)));
        setSelectedId(id);
        setReferencingProjects([]);
        setRecentRefresh(prev => prev + 1);
      } else {
        setAgents(prev => prev.map(a => a.id === id ? { ...a, name } : a).sort((a, b) => a.name.localeCompare(b.name)));
      }
      setIsCreating(false);
      setIsEditing(false);
      setDisplayName(name);
      setAgentContent(content);
      setDraft(parseSubagent(content));
      setError(null);
      setCurrentScan(toAssetSecurityScanRecord(scan));
      setSecurityNotice(warnings.length > 0 ? formatAssetScanResult(scan, "user agent") : null);
    } catch (err: unknown) {
      setError(`Failed to save agent: ${String(err)}`);
    }
  };

  const closeDrawer = () => {
    setSelectedId(null);
    setDisplayName("");
    setAgentContent("");
    setDraft(null);
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
    setAgentContent("");
    setDraft(newAgentDraft());
    setIsCreating(true);
    setIsEditing(true);
    setReferencingProjects([]);
    setSecurityNotice(null);
    setCurrentScan(null);
  };

  const handleDuplicate = async (id: string) => {
    try {
      const raw: string = await invoke("read_subagent", { machineName: id });
      const agent: UserAgent = JSON.parse(raw);
      const sourceName = parseSubagent(agent.content).fields.name || agent.name;
      const copy = duplicateSubagentIdentity({ id, name: sourceName }, agents);
      const content = withSubagentName(agent.content, copy.name);
      await invoke("save_subagent", { machineName: copy.id, name: copy.name, content });
      const newEntry: SubagentEntry = { id: copy.id, name: copy.name };
      setAgents(prev => [...prev, newEntry].sort((a, b) => a.name.localeCompare(b.name)));
      await loadAgent(copy.id);
      setIsEditing(true);
      setError(null);
      setSecurityNotice(null);
    } catch (err: unknown) {
      setError(`Failed to duplicate agent: ${String(err)}`);
    }
  };

  const selectedEntry = agents.find(a => a.id === selectedId);
  const editedContent = isEditing && draft ? serializeSubagent(trimmedDraft(draft)) : agentContent;
  const canSave = !!draft
    && validateLibraryName(draft.fields.name.trim()) === null
    && draft.fields.description.trim() !== ""
    && conflictingExtraKeys(draft).length === 0;
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
              <h3 className="text-[14px] font-medium text-text-base truncate">
                {isEditing
                  ? draft?.fields.name.trim() || (isCreating ? "New agent" : displayName)
                  : selectedEntry?.name || displayName}
              </h3>
            </div>

            <div className="flex items-center gap-2 flex-shrink-0">
              <TokenPill text={editedContent} />
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
                    disabled={!canSave}
                    title={canSave ? undefined : "Add a valid name and a description to save"}
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
            {isEditing && draft ? (
              <SubagentFieldsEditor draft={draft} onChange={setDraft} />
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
                  {draft && <SubagentFieldsView doc={draft} />}
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
                        <li key={project.local_key || project.name} className="flex items-center justify-between gap-3 py-1">
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
