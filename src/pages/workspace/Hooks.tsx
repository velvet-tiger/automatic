import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  Check,
  ChevronDown,
  Edit2,
  Plus,
  Trash2,
  Webhook,
  X,
} from "lucide-react";
import type { AgentInfo } from "../../components/AgentSelector";
import { LineNumberedTextarea } from "../../components/LineNumberedTextarea";

// ── Vendor event catalogues ────────────────────────────────────────────────
//
// These constants mirror the events accepted by each vendor's hook loader.
// They are static within a build — vendor doc updates require a recompile.
// Source: https://code.claude.com/docs/en/hooks,
// https://developers.openai.com/codex/hooks and
// https://cursor.com/docs/hooks.

const CLAUDE_CODE_EVENTS = [
  "SessionStart",
  "Setup",
  "SessionEnd",
  "UserPromptSubmit",
  "UserPromptExpansion",
  "Stop",
  "StopFailure",
  "PreToolUse",
  "PermissionRequest",
  "PermissionDenied",
  "PostToolUse",
  "PostToolUseFailure",
  "PostToolBatch",
  "SubagentStart",
  "SubagentStop",
  "TeammateIdle",
  "TaskCreated",
  "TaskCompleted",
  "FileChanged",
  "CwdChanged",
  "ConfigChange",
  "InstructionsLoaded",
  "PreCompact",
  "PostCompact",
  "Elicitation",
  "ElicitationResult",
  "Notification",
  "WorktreeCreate",
  "WorktreeRemove",
] as const;

const CODEX_CLI_EVENTS = [
  "SessionStart",
  "PreToolUse",
  "PermissionRequest",
  "PostToolUse",
  "UserPromptSubmit",
  "Stop",
] as const;

// KEEP IN LOCKSTEP with CURSOR_SUPPORTED_EVENTS in
// src-tauri/src/agent/cursor.rs — events listed here but missing there are
// silently skipped at sync time.  Cursor uses camelCase event names.
// Tab-completion hooks are deliberately excluded.
const CURSOR_EVENTS = [
  "sessionStart",
  "sessionEnd",
  "beforeSubmitPrompt",
  "preToolUse",
  "postToolUse",
  "postToolUseFailure",
  "beforeShellExecution",
  "afterShellExecution",
  "beforeMCPExecution",
  "afterMCPExecution",
  "beforeReadFile",
  "afterFileEdit",
  "stop",
  "subagentStart",
  "subagentStop",
  "preCompact",
  "afterAgentResponse",
  "afterAgentThought",
  "workspaceOpen",
] as const;

const EVENTS_BY_AGENT: Record<string, readonly string[]> = {
  claude: CLAUDE_CODE_EVENTS,
  codex: CODEX_CLI_EVENTS,
  cursor: CURSOR_EVENTS,
};

// ── Types ──────────────────────────────────────────────────────────────────

interface HookEntry {
  id: string;
  name: string;
  agent: string;
  event: string;
  plugin_id?: string | null;
}

type HookHandler =
  | { kind: "command"; command: string }
  | { kind: "script"; interpreter: string; script: string }
  | { kind: "path"; path: string; interpreter?: string | null };

interface Hook {
  name: string;
  agent: string;
  event: string;
  matcher?: string | null;
  handler: HookHandler;
  timeout_sec?: number | null;
  plugin_id?: string | null;
}

interface EditorState {
  name: string;
  agent: string;
  event: string;
  matcher: string;
  handlerKind: "command" | "script" | "path";
  command: string;
  interpreter: string;
  script: string;
  scriptPath: string;
  timeoutSec: string;
}

const DEFAULT_EDITOR: EditorState = {
  name: "",
  agent: "claude",
  event: "SessionStart",
  matcher: "",
  handlerKind: "command",
  command: "",
  interpreter: "bash",
  script: "",
  scriptPath: "",
  timeoutSec: "",
};

/** Convert a freeform display name into a machine-name slug. */
function toMachineName(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, "-")
    .replace(/-{2,}/g, "-")
    .replace(/^-+|-+$/g, "");
}

function buildEditorFromHook(id: string, hook: Hook): EditorState {
  const handler = hook.handler;
  let command = "";
  let interpreter = "bash";
  let script = "";
  let scriptPath = "";
  if (handler.kind === "command") {
    command = handler.command;
  } else if (handler.kind === "script") {
    interpreter = handler.interpreter;
    script = handler.script;
  } else if (handler.kind === "path") {
    scriptPath = handler.path;
    interpreter = handler.interpreter ?? "";
  }
  return {
    name: hook.name,
    agent: hook.agent,
    event: hook.event,
    matcher: hook.matcher ?? "",
    handlerKind: handler.kind,
    command,
    interpreter,
    script,
    scriptPath,
    timeoutSec: hook.timeout_sec != null ? String(hook.timeout_sec) : "",
    // Keep `id` referenced so editing the same hook re-loads the same slug.
    // Not stored here — handled by the parent component.
    ...({} as Partial<{ _id: string }>),
    _id: id,
  } as EditorState & { _id: string };
}

function buildPayloadFromEditor(state: EditorState): {
  name: string;
  agent: string;
  event: string;
  matcher: string | null;
  handler: HookHandler;
  timeoutSec: number | null;
} {
  const trimmedMatcher = state.matcher.trim();
  const matcher = trimmedMatcher.length > 0 ? trimmedMatcher : null;

  let handler: HookHandler;
  if (state.handlerKind === "command") {
    handler = { kind: "command", command: state.command };
  } else if (state.handlerKind === "script") {
    handler = {
      kind: "script",
      interpreter: state.interpreter.trim() || "bash",
      script: state.script,
    };
  } else {
    handler = {
      kind: "path",
      path: state.scriptPath.trim(),
      interpreter:
        state.interpreter.trim().length > 0 ? state.interpreter.trim() : null,
    };
  }

  let timeoutSec: number | null = null;
  if (state.timeoutSec.trim().length > 0) {
    const parsed = Number(state.timeoutSec);
    if (Number.isFinite(parsed) && parsed > 0) {
      timeoutSec = Math.floor(parsed);
    }
  }

  return {
    name: state.name.trim(),
    agent: state.agent,
    event: state.event,
    matcher,
    handler,
    timeoutSec,
  };
}

// ── Component ─────────────────────────────────────────────────────────────

export default function Hooks() {
  const [hooks, setHooks] = useState<HookEntry[]>([]);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState>(DEFAULT_EDITOR);
  const [editorMachineName, setEditorMachineName] = useState<string>("");
  const [isCreating, setIsCreating] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void loadHooks();
    void loadAgents();
  }, []);

  const hookCapableAgents = useMemo(
    () => agents.filter((a) => a.capabilities?.hooks),
    [agents],
  );

  const allowedEvents = useMemo(
    () => EVENTS_BY_AGENT[editor.agent] ?? [],
    [editor.agent],
  );

  async function loadHooks() {
    try {
      const result: HookEntry[] = await invoke("get_hooks");
      setHooks(result.sort((a, b) => a.id.localeCompare(b.id)));
      setError(null);
    } catch (err) {
      setError(`Failed to load hooks: ${err}`);
    }
  }

  async function loadAgents() {
    try {
      const parsed: AgentInfo[] = await invoke("list_agents");
      setAgents(parsed);
    } catch (err) {
      console.error("Failed to load agents", err);
    }
  }

  async function loadHook(id: string) {
    try {
      const raw: string = await invoke("read_hook", { machineName: id });
      const hook: Hook = JSON.parse(raw);
      const editorState = buildEditorFromHook(id, hook) as EditorState & {
        _id: string;
      };
      setSelectedId(id);
      setEditor(editorState);
      setEditorMachineName(id);
      setIsCreating(false);
      setIsEditing(false);
      setError(null);
    } catch (err) {
      setError(`Failed to read hook: ${err}`);
    }
  }

  function startCreate() {
    const defaultAgentId =
      hookCapableAgents[0]?.id ?? "claude";
    const defaultEvent =
      EVENTS_BY_AGENT[defaultAgentId]?.[0] ?? "SessionStart";
    setSelectedId(null);
    setEditor({
      ...DEFAULT_EDITOR,
      agent: defaultAgentId,
      event: defaultEvent,
    });
    setEditorMachineName("");
    setIsCreating(true);
    setIsEditing(true);
    setError(null);
  }

  function cancelEdit() {
    setIsEditing(false);
    setError(null);
    if (isCreating) {
      setIsCreating(false);
      setSelectedId(null);
      setEditor(DEFAULT_EDITOR);
      setEditorMachineName("");
    } else if (selectedId) {
      void loadHook(selectedId);
    }
  }

  async function handleSave() {
    const id = isCreating ? toMachineName(editorMachineName) : selectedId;
    if (!id) {
      setError("Provide a machine name (lowercase letters, digits, hyphens).");
      return;
    }
    const payload = buildPayloadFromEditor(editor);
    if (!payload.name) {
      setError("Display name is required.");
      return;
    }
    if (payload.handler.kind === "command" && !payload.handler.command.trim()) {
      setError("Command is required.");
      return;
    }
    if (payload.handler.kind === "script" && !payload.handler.script.trim()) {
      setError("Script body is required.");
      return;
    }
    if (payload.handler.kind === "path" && !payload.handler.path.trim()) {
      setError("Script file path is required.");
      return;
    }

    try {
      await invoke("save_hook", {
        machineName: id,
        name: payload.name,
        agent: payload.agent,
        event: payload.event,
        matcher: payload.matcher,
        handler: payload.handler,
        timeoutSec: payload.timeoutSec,
      });
      await loadHooks();
      setSelectedId(id);
      setIsCreating(false);
      setIsEditing(false);
      setError(null);
      await loadHook(id);
    } catch (err) {
      setError(`Failed to save hook: ${err}`);
    }
  }

  async function handleDelete(id: string, e: React.MouseEvent) {
    e.stopPropagation();
    const confirmed = await ask(`Delete hook "${id}"?`, {
      title: "Delete Hook",
      kind: "warning",
    });
    if (!confirmed) return;
    try {
      await invoke("delete_hook", { machineName: id });
      if (selectedId === id) {
        setSelectedId(null);
        setEditor(DEFAULT_EDITOR);
        setEditorMachineName("");
        setIsEditing(false);
      }
      await loadHooks();
    } catch (err) {
      setError(`Failed to delete hook: ${err}`);
    }
  }

  const selectedEntry = hooks.find((h) => h.id === selectedId) ?? null;
  const isPluginOwned = !!selectedEntry?.plugin_id;

  return (
    <div className="flex h-full w-full bg-bg-base">
      {/* Sidebar */}
      <div className="w-72 flex-shrink-0 flex flex-col border-r border-border-strong/40 bg-bg-input/50">
        <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center bg-bg-base/30">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Hooks
          </span>
          <button
            onClick={startCreate}
            className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
            title="Create new hook"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {hooks.length === 0 && !isCreating ? (
            <div className="px-4 py-3 text-[13px] text-text-muted text-center">
              No hooks yet.
            </div>
          ) : (
            <ul className="space-y-1 px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-bg-sidebar">
                  <div className="w-8 h-8 rounded-md bg-icon-skill/15 flex items-center justify-center flex-shrink-0">
                    <Webhook size={15} className="text-icon-skill" />
                  </div>
                  <span className="text-[13px] text-text-base italic">
                    New hook...
                  </span>
                </li>
              )}
              {hooks.map((entry) => {
                const isActive = selectedId === entry.id && !isCreating;
                return (
                  <li key={entry.id} className="group relative">
                    <button
                      onClick={() => void loadHook(entry.id)}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isActive
                          ? "bg-bg-sidebar border border-brand/30"
                          : "hover:bg-bg-sidebar/60 border border-transparent"
                      }`}
                    >
                      <div className="w-8 h-8 rounded-md bg-icon-skill/15 flex items-center justify-center flex-shrink-0">
                        <Webhook size={15} className="text-icon-skill" />
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">
                          {entry.name}
                        </div>
                        <div className="text-[11px] text-text-muted truncate mt-0.5">
                          {entry.agent} · {entry.event}
                        </div>
                      </div>
                      {!entry.plugin_id && (
                        <button
                          onClick={(e) => void handleDelete(entry.id, e)}
                          className="opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-danger/10 rounded text-text-muted hover:text-danger"
                          title="Delete"
                        >
                          <Trash2 size={12} />
                        </button>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      {/* Editor */}
      <div className="flex-1 min-w-0 flex flex-col">
        {isCreating || selectedEntry ? (
          <>
            <div className="h-11 px-5 border-b border-border-strong/40 flex items-center justify-between bg-bg-base/30">
              <div className="flex items-center gap-3 min-w-0">
                <div className="w-8 h-8 rounded-md bg-icon-skill/15 flex items-center justify-center flex-shrink-0">
                  <Webhook size={15} className="text-icon-skill" />
                </div>
                <div className="min-w-0">
                  <div className="text-[15px] font-semibold text-text-base truncate">
                    {isCreating ? "New hook" : selectedEntry?.name}
                  </div>
                  <div className="text-[11px] text-text-muted">
                    {isCreating
                      ? "Define the event and handler"
                      : `${selectedEntry?.agent} · ${selectedEntry?.event}`}
                  </div>
                </div>
              </div>
              <div className="flex items-center gap-2">
                {!isEditing && !isPluginOwned && (
                  <button
                    onClick={() => setIsEditing(true)}
                    className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-border-strong/50 text-[12px] text-text-base hover:bg-bg-sidebar transition-colors"
                  >
                    <Edit2 size={12} /> Edit
                  </button>
                )}
                {isEditing && (
                  <>
                    <button
                      onClick={cancelEdit}
                      className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-border-strong/50 text-[12px] text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors"
                    >
                      <X size={12} /> Cancel
                    </button>
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

            <div className="flex-1 min-h-0 overflow-y-auto custom-scrollbar p-6 space-y-5">
              {isPluginOwned && (
                <div className="rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3 text-[12px] text-text-muted">
                  This hook is provided by a plugin and cannot be edited from
                  here. Remove it by uninstalling the owning plugin.
                </div>
              )}

              {/* Display name */}
              <Field label="Display name" required>
                <input
                  type="text"
                  value={editor.name}
                  onChange={(e) =>
                    setEditor((prev) => ({ ...prev, name: e.target.value }))
                  }
                  readOnly={!isEditing}
                  placeholder="e.g. Log session start"
                  className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 outline-none text-[13px] text-text-base placeholder-text-muted/40 focus:border-brand transition-colors"
                />
              </Field>

              {/* Machine name (creation only) */}
              {isCreating && (
                <Field label="Machine name" required>
                  <input
                    type="text"
                    value={editorMachineName}
                    onChange={(e) =>
                      setEditorMachineName(toMachineName(e.target.value))
                    }
                    placeholder="log-session-start"
                    className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 outline-none text-[13px] text-text-base placeholder-text-muted/40 focus:border-brand font-mono transition-colors"
                  />
                  <p className="mt-1 text-[11px] text-text-muted">
                    Lowercase letters, digits and hyphens. Cannot be changed
                    later.
                  </p>
                </Field>
              )}

              {/* Target agent */}
              <Field label="Target agent" required>
                <Dropdown
                  value={editor.agent}
                  disabled={!isEditing}
                  onChange={(value) => {
                    const events = EVENTS_BY_AGENT[value] ?? [];
                    setEditor((prev) => ({
                      ...prev,
                      agent: value,
                      event: events.includes(prev.event)
                        ? prev.event
                        : events[0] ?? prev.event,
                    }));
                  }}
                  options={hookCapableAgents.map((a) => ({
                    value: a.id,
                    label: a.label,
                  }))}
                  ariaLabel="Target agent"
                />
              </Field>

              {/* Event */}
              <Field label="Event" required>
                <Dropdown
                  value={editor.event}
                  disabled={!isEditing}
                  onChange={(value) =>
                    setEditor((prev) => ({ ...prev, event: value }))
                  }
                  options={allowedEvents.map((e) => ({
                    value: e,
                    label: e,
                  }))}
                  ariaLabel="Event"
                />
              </Field>

              {/* Matcher (optional) */}
              <Field label="Matcher (optional)">
                <input
                  type="text"
                  value={editor.matcher}
                  onChange={(e) =>
                    setEditor((prev) => ({ ...prev, matcher: e.target.value }))
                  }
                  readOnly={!isEditing}
                  placeholder='e.g. Bash or "Bash|Edit" for PreToolUse'
                  className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 outline-none text-[13px] text-text-base placeholder-text-muted/40 focus:border-brand font-mono transition-colors"
                />
                <p className="mt-1 text-[11px] text-text-muted">
                  Only used by events that support filtering (e.g.
                  PreToolUse/PostToolUse).
                </p>
              </Field>

              {/* Handler kind */}
              <Field label="Handler" required>
                <div className="inline-flex rounded-md border border-border-strong/40 overflow-hidden">
                  {(
                    [
                      { kind: "command", label: "Inline command" },
                      { kind: "script", label: "Inline script" },
                      { kind: "path", label: "Script file" },
                    ] as const
                  ).map(({ kind, label }) => {
                    const active = editor.handlerKind === kind;
                    return (
                      <button
                        key={kind}
                        type="button"
                        disabled={!isEditing}
                        onClick={() =>
                          setEditor((prev) => ({ ...prev, handlerKind: kind }))
                        }
                        className={`px-3 py-1.5 text-[12px] transition-colors ${
                          active
                            ? "bg-brand text-white"
                            : "bg-bg-sidebar text-text-muted hover:text-text-base"
                        } ${!isEditing ? "opacity-70 cursor-default" : ""}`}
                      >
                        {label}
                      </button>
                    );
                  })}
                </div>
                <p className="mt-1.5 text-[11px] text-text-muted">
                  {editor.handlerKind === "command" &&
                    "Run a single shell snippet directly."}
                  {editor.handlerKind === "script" &&
                    "Write a script body here — Automatic writes it to .claude/hooks/ (or the agent's equivalent) and the hook references that file."}
                  {editor.handlerKind === "path" &&
                    "Point at a script file you already have on disk. Automatic does not write or own the file. You can use placeholders like ${CLAUDE_PROJECT_DIR}."}
                </p>
              </Field>

              {/* Handler body */}
              {editor.handlerKind === "command" && (
                <Field label="Command" required>
                  <textarea
                    value={editor.command}
                    onChange={(e) =>
                      setEditor((prev) => ({ ...prev, command: e.target.value }))
                    }
                    readOnly={!isEditing}
                    rows={3}
                    placeholder='echo "session $CLAUDE_SESSION_ID started"'
                    className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 outline-none text-[13px] text-text-base placeholder-text-muted/40 font-mono leading-relaxed resize-y focus:border-brand transition-colors"
                  />
                </Field>
              )}
              {editor.handlerKind === "script" && (
                <>
                  <Field label="Interpreter" required>
                    <input
                      type="text"
                      value={editor.interpreter}
                      onChange={(e) =>
                        setEditor((prev) => ({
                          ...prev,
                          interpreter: e.target.value,
                        }))
                      }
                      readOnly={!isEditing}
                      placeholder="bash"
                      className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 outline-none text-[13px] text-text-base placeholder-text-muted/40 font-mono focus:border-brand transition-colors"
                    />
                    <p className="mt-1 text-[11px] text-text-muted">
                      Used to generate the shebang line on disk if your script
                      doesn't already have one.
                    </p>
                  </Field>
                  <Field label="Script" required>
                    <LineNumberedTextarea
                      value={editor.script}
                      onChange={(value) =>
                        isEditing
                          ? setEditor((prev) => ({ ...prev, script: value }))
                          : undefined
                      }
                      className="h-64"
                    />
                  </Field>
                </>
              )}
              {editor.handlerKind === "path" && (
                <>
                  <Field label="Script file path" required>
                    <input
                      type="text"
                      value={editor.scriptPath}
                      onChange={(e) =>
                        setEditor((prev) => ({
                          ...prev,
                          scriptPath: e.target.value,
                        }))
                      }
                      readOnly={!isEditing}
                      placeholder="${CLAUDE_PROJECT_DIR}/scripts/log-session.sh"
                      className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 outline-none text-[13px] text-text-base placeholder-text-muted/40 font-mono focus:border-brand transition-colors"
                    />
                    <p className="mt-1 text-[11px] text-text-muted">
                      Absolute path, or a vendor placeholder like
                      <code className="px-1">${"{CLAUDE_PROJECT_DIR}"}</code>.
                      The file must be executable.
                    </p>
                  </Field>
                  <Field label="Interpreter (optional)">
                    <input
                      type="text"
                      value={editor.interpreter}
                      onChange={(e) =>
                        setEditor((prev) => ({
                          ...prev,
                          interpreter: e.target.value,
                        }))
                      }
                      readOnly={!isEditing}
                      placeholder="leave blank to use the file's shebang"
                      className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 outline-none text-[13px] text-text-base placeholder-text-muted/40 font-mono focus:border-brand transition-colors"
                    />
                  </Field>
                </>
              )}

              {/* Timeout */}
              <Field label="Timeout (seconds, optional)">
                <input
                  type="number"
                  min={1}
                  value={editor.timeoutSec}
                  onChange={(e) =>
                    setEditor((prev) => ({
                      ...prev,
                      timeoutSec: e.target.value,
                    }))
                  }
                  readOnly={!isEditing}
                  placeholder="60"
                  className="w-32 px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 outline-none text-[13px] text-text-base placeholder-text-muted/40 focus:border-brand transition-colors"
                />
              </Field>
            </div>
          </>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-16 h-16 mx-auto mb-6 rounded-2xl bg-icon-skill/12 border border-icon-skill/20 flex items-center justify-center">
              <Webhook size={24} className="text-icon-skill" strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-text-base mb-2">
              {hooks.length === 0 ? "No hooks yet" : "No hook selected"}
            </h2>
            <p className="text-[14px] text-text-muted mb-8 leading-relaxed max-w-sm">
              {hooks.length === 0
                ? "Hooks run shell commands or scripts on agent lifecycle events. Create one to react to session starts, tool use, prompt submission, and more."
                : "Select a hook from the list to view or edit it, or create a new one."}
            </p>
            <button
              onClick={startCreate}
              className="px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              Create hook
            </button>
          </div>
        )}

        {error && (
          <div className="mx-5 mb-5 mt-0 rounded-lg border border-red-300/80 bg-red-50 px-4 py-3 text-[12px] text-red-950">
            <div className="whitespace-pre-wrap">{error}</div>
          </div>
        )}
      </div>
    </div>
  );
}

// ── Small reusable controls ────────────────────────────────────────────────

function Field({
  label,
  required,
  children,
}: {
  label: string;
  required?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1.5">
        {label}
        {required && <span className="text-red-400 ml-0.5">*</span>}
      </label>
      {children}
    </div>
  );
}

function Dropdown({
  value,
  options,
  onChange,
  disabled,
  ariaLabel,
}: {
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  disabled?: boolean;
  ariaLabel: string;
}) {
  return (
    <div className="relative">
      <select
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        className="w-full appearance-none text-[12px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 pr-7 py-2 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors disabled:opacity-70"
        aria-label={ariaLabel}
      >
        {options.length === 0 && <option value={value}>{value}</option>}
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      <ChevronDown
        size={12}
        className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted"
      />
    </div>
  );
}
