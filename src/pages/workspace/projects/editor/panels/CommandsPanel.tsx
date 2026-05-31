// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { invoke } from "@tauri-apps/api/core";
import { Check, ChevronRight, Edit2, ExternalLink, Globe, Plus, Terminal, Trash2, X } from "lucide-react";
import { LineNumberedTextarea } from "../../../../../components/LineNumberedTextarea";
import { MarkdownPreview } from "../../../../../components/MarkdownPreview";
import { TokenPill } from "../../../../../components/TokenPill";
import type { CustomCommand, Project, UserCommandEntry } from "../../types";

interface CommandsPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  dirty: boolean;
  syncStatus: string | null;
  handleSave: () => void | Promise<void>;
  customCommandEditingIdx: number | null;
  setCustomCommandEditingIdx: (v: number | null) => void;
  customCommandEditName: string;
  setCustomCommandEditName: (v: string) => void;
  customCommandEditContent: string;
  setCustomCommandEditContent: (v: string) => void;
  availableUserCommands: UserCommandEntry[];
  userCommandAdding: boolean;
  setUserCommandAdding: (v: boolean) => void;
  userCommandSearch: string;
  setUserCommandSearch: (v: string) => void;
  expandedCommandId: string | null;
  setExpandedCommandId: (v: string | null) => void;
  expandedCommandContent: string;
  setExpandedCommandContent: (v: string) => void;
  expandedCommandError: string | null;
  setExpandedCommandError: (v: string | null) => void;
  expandedCommandLoading: boolean;
  setExpandedCommandLoading: (v: boolean) => void;
  onNavigateToCommand?: (commandId: string) => void;
}

export function CommandsPanel(props: CommandsPanelProps) {
  const {
    project, setProject, setDirty, dirty, syncStatus, handleSave,
    customCommandEditingIdx, setCustomCommandEditingIdx,
    customCommandEditName, setCustomCommandEditName,
    customCommandEditContent, setCustomCommandEditContent,
    availableUserCommands,
    userCommandAdding, setUserCommandAdding,
    userCommandSearch, setUserCommandSearch,
    expandedCommandId, setExpandedCommandId,
    expandedCommandContent, setExpandedCommandContent,
    expandedCommandError, setExpandedCommandError,
    expandedCommandLoading, setExpandedCommandLoading,
    onNavigateToCommand,
  } = props;

  const customCommands: CustomCommand[] = project.custom_commands || [];

  const handleAddCustomCommand = () => {
    const newCommand: CustomCommand = {
      name: "new-command",
      content: "---\ndescription: Describe what this command does.\n---\n\nWrite the reusable prompt here.\n",
    };
    setProject({ ...project, custom_commands: [...customCommands, newCommand] });
    setCustomCommandEditingIdx(customCommands.length);
    setCustomCommandEditName(newCommand.name);
    setCustomCommandEditContent(newCommand.content);
    setDirty(true);
  };

  const handleDeleteCustomCommand = (idx: number) => {
    const updated = customCommands.filter((_, i) => i !== idx);
    setProject({ ...project, custom_commands: updated.length > 0 ? updated : undefined });
    if (customCommandEditingIdx === idx) {
      setCustomCommandEditingIdx(null);
    } else if (customCommandEditingIdx !== null && customCommandEditingIdx > idx) {
      setCustomCommandEditingIdx(customCommandEditingIdx - 1);
    }
    setDirty(true);
  };

  const handleStartEditCustomCommand = (idx: number) => {
    setCustomCommandEditingIdx(idx);
    setCustomCommandEditName(customCommands[idx]?.name ?? "");
    setCustomCommandEditContent(customCommands[idx]?.content ?? "");
  };

  const handleCommitCustomCommand = () => {
    if (customCommandEditingIdx === null) return;
    const updated = customCommands.map((command, i) =>
      i === customCommandEditingIdx
        ? {
            name: customCommandEditName.trim() || "untitled-command",
            content: customCommandEditContent,
          }
        : command
    );
    setProject({ ...project, custom_commands: updated });
    setCustomCommandEditingIdx(null);
    setDirty(true);
  };

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-[15px] font-semibold text-text-base">Commands</h2>
        </div>
        {((project.user_commands?.length ?? 0) + customCommands.length) > 0 && (
          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
            {(project.user_commands?.length ?? 0) + customCommands.length} commands
          </span>
        )}
      </div>

      <section>
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Terminal size={13} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Commands</span>
            {customCommands.length > 0 && (
              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                {customCommands.length}
              </span>
            )}
          </div>
          <button
            onClick={handleAddCustomCommand}
            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
          >
            <Plus size={12} /> Add Command
          </button>
        </div>

        {customCommands.length === 0 ? (
          <button
            onClick={handleAddCustomCommand}
            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
          >
            <Plus size={14} /> Create your first project command
          </button>
        ) : (
          <div className="space-y-2">
            {customCommands.map((command, idx) => {
              const isEditing = customCommandEditingIdx === idx;
              return (
                <div
                  key={`${command.name}-${idx}`}
                  className={`rounded-lg border transition-colors ${
                    isEditing
                      ? "border-brand/40 bg-bg-input"
                      : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                  }`}
                >
                  {isEditing ? (
                    <div className="p-3 space-y-2">
                      <input
                        type="text"
                        value={customCommandEditName}
                        onChange={(e) => setCustomCommandEditName(e.target.value)}
                        placeholder="command-name"
                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                      />
                      <LineNumberedTextarea
                        value={customCommandEditContent}
                        onChange={setCustomCommandEditContent}
                        placeholder="Write the command as Markdown with optional YAML frontmatter..."
                        variant="inline"
                        rows={12}
                        className="w-full"
                      />
                      <div className="flex items-center justify-end gap-2 pt-1">
                        <button
                          onClick={() => setCustomCommandEditingIdx(null)}
                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                        >
                          Cancel
                        </button>
                        <button
                          onClick={handleCommitCustomCommand}
                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                        >
                          <Check size={11} /> Save
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-center gap-3 px-3 py-2.5">
                      <Terminal size={14} className="flex-shrink-0 text-text-muted" />
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">/{command.name || "untitled-command"}</div>
                        <div className="text-[11px] text-text-muted truncate mt-0.5">
                          {command.content.trim().split("\n").find((line) => line.trim() && !line.startsWith("---"))?.slice(0, 80) || "Custom command"}
                        </div>
                      </div>
                      <TokenPill text={command.content} />
                      <div className="flex items-center gap-1 flex-shrink-0">
                        <button
                          onClick={() => handleStartEditCustomCommand(idx)}
                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                          title="Edit"
                        >
                          <Edit2 size={12} />
                        </button>
                        <button
                          onClick={() => handleDeleteCustomCommand(idx)}
                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                          title="Delete"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>

      <section>
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <div className="p-1 bg-icon-agent/10 rounded"><Globe size={12} className="text-icon-agent" /></div>
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Workspace Commands</span>
            {(project.user_commands?.length ?? 0) > 0 && (
              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                {project.user_commands?.length ?? 0}
              </span>
            )}
          </div>
          <div className="relative">
            <button
              onClick={() => setUserCommandAdding(!userCommandAdding)}
              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
            >
              <Plus size={12} /> Add from Library
            </button>
            {userCommandAdding && (
              <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
                <div className="p-2 border-b border-border-strong/40">
                  <input
                    type="text"
                    value={userCommandSearch}
                    onChange={(e) => setUserCommandSearch(e.target.value)}
                    placeholder="Search commands..."
                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                    autoFocus
                  />
                </div>
                <div className="py-1">
                  {availableUserCommands
                    .filter((command) => {
                      const search = userCommandSearch.toLowerCase();
                      return (
                        command.id.toLowerCase().includes(search) ||
                        command.description.toLowerCase().includes(search)
                      );
                    })
                    .filter((command) => !(project.user_commands ?? []).includes(command.id))
                    .length === 0 ? (
                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                      {availableUserCommands.length === 0
                        ? "No workspace commands available"
                        : "All commands already added"}
                    </div>
                  ) : (
                    availableUserCommands
                      .filter((command) => {
                        const search = userCommandSearch.toLowerCase();
                        return (
                          command.id.toLowerCase().includes(search) ||
                          command.description.toLowerCase().includes(search)
                        );
                      })
                      .filter((command) => !(project.user_commands ?? []).includes(command.id))
                      .map((command) => (
                        <button
                          key={command.id}
                          onClick={() => {
                            const currentUserCommands = project.user_commands ?? [];
                            setProject({
                              ...project,
                              user_commands: [...currentUserCommands, command.id],
                            });
                            setDirty(true);
                            setUserCommandAdding(false);
                            setUserCommandSearch("");
                          }}
                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                        >
                          <Terminal size={14} className="text-text-muted flex-shrink-0" />
                          <div className="min-w-0">
                            <div className="text-[12px] font-medium text-text-base truncate">
                              /{command.id}
                            </div>
                            <div className="text-[11px] text-text-muted truncate">
                              {command.description || "No description"}
                            </div>
                          </div>
                        </button>
                      ))
                  )}
                </div>
              </div>
            )}
          </div>
        </div>

        {(project.user_commands?.length ?? 0) === 0 ? (
          <div className="text-[12px] text-text-muted/60 italic py-4 text-center">
            No workspace commands selected. Add commands from your library to include them in this project.
          </div>
        ) : (
          <div className="space-y-2">
            {project.user_commands?.map((commandId) => {
              const command = availableUserCommands.find((entry) => entry.id === commandId);
              const isExpanded = expandedCommandId === commandId;

              const handleToggleExpandCommand = async () => {
                if (isExpanded) {
                  setExpandedCommandId(null);
                  setExpandedCommandContent("");
                  setExpandedCommandError(null);
                  return;
                }
                setExpandedCommandId(commandId);
                setExpandedCommandContent("");
                setExpandedCommandError(null);
                setExpandedCommandLoading(true);
                try {
                  const raw: string = await invoke("read_user_command", { machineName: commandId });
                  setExpandedCommandContent(raw);
                } catch (err: unknown) {
                  setExpandedCommandError(String(err));
                } finally {
                  setExpandedCommandLoading(false);
                }
              };

              const extractCommandBody = (raw: string): string => {
                const match = raw.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?([\s\S]*)$/);
                return match ? match[1]!.trimStart() : raw;
              };

              return (
                <div
                  key={commandId}
                  className={`bg-bg-input border rounded-lg group transition-colors ${
                    isExpanded ? "border-brand/40" : "border-border-strong/40"
                  }`}
                >
                  <div className="flex items-center gap-3 px-3 py-2.5">
                    <Terminal size={14} className="flex-shrink-0 text-text-muted" />
                    <button
                      className="flex-1 flex items-center gap-2 text-left min-w-0"
                      onClick={handleToggleExpandCommand}
                    >
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">
                          /{command?.id ?? commandId}
                        </div>
                        <div className="text-[11px] text-text-muted truncate">
                          {command?.description || commandId}
                        </div>
                      </div>
                      <ChevronRight
                        size={12}
                        className={`text-text-muted flex-shrink-0 transition-transform ${isExpanded ? "rotate-90" : ""}`}
                      />
                    </button>
                    <button
                      onClick={() => {
                        const updated = (project.user_commands ?? []).filter((id) => id !== commandId);
                        setProject({ ...project, user_commands: updated.length > 0 ? updated : undefined });
                        setDirty(true);
                        if (isExpanded) {
                          setExpandedCommandId(null);
                          setExpandedCommandContent("");
                        }
                      }}
                      className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 opacity-0 group-hover:opacity-100"
                      title="Remove"
                    >
                      <X size={12} />
                    </button>
                  </div>

                  {isExpanded && (
                    <div className="border-t border-border-strong/40">
                      {onNavigateToCommand && (
                        <div className="flex items-center gap-3 px-3 py-2 border-b border-border-strong/30 bg-bg-sidebar/30">
                          <button
                            onClick={() => onNavigateToCommand(commandId)}
                            className="flex items-center gap-1 text-[11px] text-text-muted hover:text-brand transition-colors"
                            title="View this command in the Commands library"
                          >
                            <ExternalLink size={11} />
                            View in library
                          </button>
                        </div>
                      )}

                      <div className="px-4 py-3 max-h-80 overflow-y-auto custom-scrollbar">
                        {expandedCommandLoading && (
                          <p className="text-[12px] text-text-muted italic">Loading…</p>
                        )}
                        {expandedCommandError && (
                          <p className="text-[12px] text-danger">{expandedCommandError}</p>
                        )}
                        {!expandedCommandLoading && !expandedCommandError && expandedCommandContent && (
                          <MarkdownPreview content={extractCommandBody(expandedCommandContent)} />
                        )}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>

      {dirty && (
        <div className="flex justify-end">
          <button
            onClick={handleSave}
            disabled={syncStatus === "syncing"}
            className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
          >
            <Check size={13} /> {syncStatus === "syncing" ? "Saving..." : "Save Changes"}
          </button>
        </div>
      )}
    </div>
  );
}
