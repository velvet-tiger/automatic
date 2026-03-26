import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, Terminal, Trash2, Check, Edit2, X } from "lucide-react";
import { TokenPill } from "../../components/TokenPill";

interface UserCommandEntry {
  id: string;
  description: string;
}

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
  const safeDesc = description.includes(":") ? `"${description.replace(/"/g, '\\"')}"` : description;
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
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editDescription, setEditDescription] = useState("");
  const [editBody, setEditBody] = useState("");
  const [descriptionError, setDescriptionError] = useState<string | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newMachineName, setNewMachineName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");

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
      const { description, body } = parseCommandContent(raw);
      setSelectedId(id);
      setEditDescription(description);
      setEditBody(body);
      setDescriptionError(null);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
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
      await invoke("save_user_command", { machineName: id, content });
      await loadCommands();
      setSelectedId(id);
      setIsCreating(false);
      setIsEditing(false);
      setDescriptionError(null);
      setError(null);
    } catch (err: any) {
      setError(`Failed to save command: ${err}`);
    }
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete command "${id}"?`, { title: "Delete Command", kind: "warning" });
    if (!confirmed) return;

    try {
      await invoke("delete_user_command", { machineName: id });
      if (selectedId === id) {
        setSelectedId(null);
        setEditDescription("");
        setEditBody("");
        setIsEditing(false);
      }
      await loadCommands();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete command: ${err}`);
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
    } catch (err: any) {
      setError(`Failed to rename command: ${err}`);
    }
  };

  const selectedEntry = commands.find((entry) => entry.id === selectedId) ?? null;

  return (
    <div className="flex h-full w-full bg-bg-base">
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-border-strong/40 bg-bg-input/50">
        <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center bg-bg-base/30">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Commands</span>
          <button
            onClick={startCreateNew}
            className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
            title="Create New Command"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {commands.length === 0 && !isCreating ? (
            <div className="px-4 py-3 text-[13px] text-text-muted text-center">No commands yet.</div>
          ) : (
            <ul className="space-y-1 px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-bg-sidebar">
                  <div className="w-8 h-8 rounded-md bg-icon-agent/15 flex items-center justify-center flex-shrink-0">
                    <Terminal size={15} className="text-icon-agent" />
                  </div>
                  <span className="text-[13px] text-text-base italic">New Command...</span>
                </li>
              )}
              {commands.map((entry) => {
                const isActive = selectedId === entry.id && !isCreating;
                return (
                  <li key={entry.id} className="group relative">
                    <button
                      onClick={() => void loadCommand(entry.id)}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isActive ? "bg-bg-sidebar border border-brand/30" : "hover:bg-bg-sidebar/60 border border-transparent"
                      }`}
                    >
                      <div className="w-8 h-8 rounded-md bg-icon-agent/15 flex items-center justify-center flex-shrink-0">
                        <Terminal size={15} className="text-icon-agent" />
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">/{entry.id}</div>
                        <div className="text-[11px] text-text-muted truncate mt-0.5">
                          {entry.description || "No description"}
                        </div>
                      </div>
                      <button
                        onClick={(e) => void handleDelete(entry.id, e)}
                        className="opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-danger/10 rounded text-text-muted hover:text-danger"
                        title="Delete"
                      >
                        <Trash2 size={12} />
                      </button>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      <div className="flex-1 min-w-0 flex flex-col">
        {isCreating || selectedEntry ? (
          <>
            <div className="h-11 px-5 border-b border-border-strong/40 flex items-center justify-between bg-bg-base/30">
              <div className="flex items-center gap-3 min-w-0">
                <div className="w-8 h-8 rounded-md bg-icon-agent/15 flex items-center justify-center flex-shrink-0">
                  <Terminal size={15} className="text-icon-agent" />
                </div>
                <div className="min-w-0">
                  {isCreating ? (
                    <input
                      type="text"
                      value={newMachineName}
                      onChange={(e) => setNewMachineName(toCommandName(e.target.value))}
                      placeholder="command-name"
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
              <div className="flex items-center gap-2">
                {!isEditing ? (
                  <button
                    onClick={() => setIsEditing(true)}
                    className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-border-strong/50 text-[12px] text-text-base hover:bg-bg-sidebar transition-colors"
                  >
                    <Edit2 size={12} /> Edit
                  </button>
                ) : (
                  <>
                    <button
                      onClick={() => {
                        setIsEditing(false);
                        setDescriptionError(null);
                        if (selectedId) void loadCommand(selectedId);
                        if (isCreating) {
                          setIsCreating(false);
                          setEditDescription("");
                          setEditBody("");
                        }
                      }}
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

            <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
              {isEditing ? (
                <>
                  {/* Frontmatter fields */}
                  <div className="px-6 pt-5 pb-4 border-b border-border-strong/40 shrink-0 space-y-4">
                    {/* description */}
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
                    <textarea
                      value={editBody}
                      onChange={(e) => setEditBody(e.target.value)}
                      className="flex-1 px-6 pb-6 resize-none outline-none font-mono text-[13px] bg-bg-base text-text-base leading-relaxed custom-scrollbar"
                      spellCheck={false}
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
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center text-text-muted text-[13px]">
            Select a command or create a new one.
          </div>
        )}

        {error && (
          <div className="mx-5 mb-5 mt-0 rounded-lg border border-danger/30 bg-danger/10 px-4 py-3 text-[12px] text-danger">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}
