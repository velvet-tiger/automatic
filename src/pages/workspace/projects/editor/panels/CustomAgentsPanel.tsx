// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import type { RefObject } from "react";
import { Check, Edit2, Globe, MessagesSquare, Plus, Trash2, X } from "lucide-react";
import { LineNumberedTextarea } from "../../../../../components/LineNumberedTextarea";
import { TokenPill } from "../../../../../components/TokenPill";
import type { CustomAgent, Project, SubagentEntry } from "../../types";

interface CustomAgentsPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  dirty: boolean;
  syncStatus: string | null;
  handleSave: () => void | Promise<void>;
  customAgentEditingIdx: number | null;
  setCustomAgentEditingIdx: (v: number | null) => void;
  customAgentEditName: string;
  setCustomAgentEditName: (v: string) => void;
  customAgentEditContent: string;
  setCustomAgentEditContent: (v: string) => void;
  availableUserAgents: SubagentEntry[];
  userAgentAdding: boolean;
  setUserAgentAdding: (v: boolean) => void;
  userAgentSearch: string;
  setUserAgentSearch: (v: string) => void;
  userAgentDropdownRef: RefObject<HTMLDivElement | null>;
}

export function CustomAgentsPanel(props: CustomAgentsPanelProps) {
  const {
    project, setProject, setDirty, dirty, syncStatus, handleSave,
    customAgentEditingIdx, setCustomAgentEditingIdx,
    customAgentEditName, setCustomAgentEditName,
    customAgentEditContent, setCustomAgentEditContent,
    availableUserAgents,
    userAgentAdding, setUserAgentAdding,
    userAgentSearch, setUserAgentSearch,
    userAgentDropdownRef,
  } = props;

  const customAgents: CustomAgent[] = project.custom_agents || [];

  const handleAddCustomAgent = () => {
    const newAgent: CustomAgent = {
      name: "New Agent",
      content: "---\nname: new-agent\ndescription: A specialized AI assistant.\ntools: Read, Grep, Glob, Bash\nmodel: inherit\n---\n\nYou are a specialized AI assistant.\n"
    };
    setProject({ ...project, custom_agents: [...customAgents, newAgent] });
    setCustomAgentEditingIdx(customAgents.length);
    setCustomAgentEditName("New Agent");
    setCustomAgentEditContent(newAgent.content);
    setDirty(true);
  };

  const handleDeleteCustomAgent = (idx: number) => {
    const updated = customAgents.filter((_, i) => i !== idx);
    setProject({ ...project, custom_agents: updated.length > 0 ? updated : undefined });
    if (customAgentEditingIdx === idx) {
      setCustomAgentEditingIdx(null);
    } else if (customAgentEditingIdx !== null && customAgentEditingIdx > idx) {
      setCustomAgentEditingIdx(customAgentEditingIdx - 1);
    }
    setDirty(true);
  };

  const handleStartEditCustomAgent = (idx: number) => {
    setCustomAgentEditingIdx(idx);
    setCustomAgentEditName(customAgents[idx]?.name ?? "");
    setCustomAgentEditContent(customAgents[idx]?.content ?? "");
  };

  const handleCommitCustomAgent = () => {
    if (customAgentEditingIdx === null) return;
    const updated = customAgents.map((a, i) =>
      i === customAgentEditingIdx
        ? { name: customAgentEditName.trim() || "Untitled Agent", content: customAgentEditContent }
        : a
    );
    setProject({ ...project, custom_agents: updated });
    setCustomAgentEditingIdx(null);
    setDirty(true);
  };

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-[15px] font-semibold text-text-base">Agents</h2>
        </div>
        {customAgents.length > 0 && (
          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
            {customAgents.length} {customAgents.length === 1 ? "agent" : "agents"}
          </span>
        )}
      </div>

      {/* ── Custom Agents ── */}
      <section>
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <MessagesSquare size={13} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Agents</span>
            {customAgents.length > 0 && (
              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                {customAgents.length}
              </span>
            )}
          </div>
          <button
            onClick={handleAddCustomAgent}
            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
          >
            <Plus size={12} /> Add Agent
          </button>
        </div>

        {customAgents.length === 0 ? (
          <button
            onClick={handleAddCustomAgent}
            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
          >
            <Plus size={14} /> Create your first project agent
          </button>
        ) : (
          <div className="space-y-2">
            {customAgents.map((agent, idx) => {
              const isEditing = customAgentEditingIdx === idx;
              return (
                <div
                  key={idx}
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
                        value={customAgentEditName}
                        onChange={(e) => setCustomAgentEditName(e.target.value)}
                        placeholder="Agent display name"
                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                      />
                      <LineNumberedTextarea
                        value={customAgentEditContent}
                        onChange={setCustomAgentEditContent}
                        placeholder="Write the agent content as Markdown with YAML frontmatter..."
                        variant="inline"
                        rows={12}
                        className="w-full"
                      />
                      <div className="flex items-center justify-end gap-2 pt-1">
                        <button
                          onClick={() => setCustomAgentEditingIdx(null)}
                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                        >
                          Cancel
                        </button>
                        <button
                          onClick={handleCommitCustomAgent}
                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                        >
                          <Check size={11} /> Save
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-center gap-3 px-3 py-2.5">
                      <MessagesSquare size={14} className="flex-shrink-0 text-text-muted" />
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">{agent.name || "Untitled Agent"}</div>
                        {agent.content.trim() ? (
                          <div className="text-[11px] text-text-muted truncate mt-0.5">
                            {agent.content.trim().split("\n").find(l => l.trim() && !l.startsWith("---"))?.slice(0, 60) || "Custom agent"}
                          </div>
                        ) : (
                          <div className="text-[11px] text-text-muted/60 italic mt-0.5">Empty</div>
                        )}
                      </div>
                      <TokenPill text={agent.content} />
                      <div className="flex items-center gap-1 flex-shrink-0">
                        <button
                          onClick={() => handleStartEditCustomAgent(idx)}
                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                          title="Edit"
                        >
                          <Edit2 size={12} />
                        </button>
                        <button
                          onClick={() => handleDeleteCustomAgent(idx)}
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

      {/* ── Workspace Agents (from ~/.automatic/agents/) ── */}
      <section>
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <div className="p-1 bg-icon-agent/10 rounded"><Globe size={12} className="text-icon-agent" /></div>
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Workspace Agents</span>
            {(project.user_agents?.length ?? 0) > 0 && (
              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                {project.user_agents?.length ?? 0}
              </span>
            )}
          </div>
          <div className="relative" ref={userAgentDropdownRef}>
            <button
              onClick={() => setUserAgentAdding(!userAgentAdding)}
              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
            >
              <Plus size={12} /> Add from Library
            </button>
            {userAgentAdding && (
              <div className="absolute right-0 top-full mt-1 w-64 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-64 overflow-y-auto">
                <div className="p-2 border-b border-border-strong/40">
                  <input
                    type="text"
                    value={userAgentSearch}
                    onChange={(e) => setUserAgentSearch(e.target.value)}
                    placeholder="Search agents..."
                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                    autoFocus
                  />
                </div>
                <div className="py-1">
                  {availableUserAgents
                    .filter((a) => {
                      const search = userAgentSearch.toLowerCase();
                      return (
                        a.name.toLowerCase().includes(search) ||
                        a.id.toLowerCase().includes(search)
                      );
                    })
                    .filter((a) => !(project.user_agents ?? []).includes(a.id))
                    .length === 0 ? (
                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                      {availableUserAgents.length === 0
                        ? "No workspace agents available"
                        : "All agents already added"}
                    </div>
                  ) : (
                    availableUserAgents
                      .filter((a) => {
                        const search = userAgentSearch.toLowerCase();
                        return (
                          a.name.toLowerCase().includes(search) ||
                          a.id.toLowerCase().includes(search)
                        );
                      })
                      .filter((a) => !(project.user_agents ?? []).includes(a.id))
                      .map((agent) => (
                        <button
                          key={agent.id}
                          onClick={() => {
                            const currentUserAgents = project.user_agents ?? [];
                            setProject({
                              ...project,
                              user_agents: [...currentUserAgents, agent.id],
                            });
                            setDirty(true);
                            setUserAgentAdding(false);
                            setUserAgentSearch("");
                          }}
                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                        >
                          <MessagesSquare size={14} className="text-text-muted flex-shrink-0" />
                          <div className="min-w-0">
                            <div className="text-[12px] font-medium text-text-base truncate">
                              {agent.name}
                            </div>
                            <div className="text-[11px] text-text-muted truncate">
                              {agent.id}
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

        {(project.user_agents?.length ?? 0) === 0 ? (
          <div className="text-[12px] text-text-muted/60 italic py-4 text-center">
            No workspace agents selected. Add agents from your library to include them in this project.
          </div>
        ) : (
          <div className="space-y-1">
            {project.user_agents?.map((agentId) => {
              const agent = availableUserAgents.find((a) => a.id === agentId);
              return (
                <div
                  key={agentId}
                  className="flex items-center gap-3 px-3 py-2 bg-bg-input border border-border-strong/40 hover:border-border-strong rounded-lg transition-colors"
                >
                  <MessagesSquare size={14} className="flex-shrink-0 text-text-muted" />
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] font-medium text-text-base truncate">
                      {agent?.name ?? agentId}
                    </div>
                    <div className="text-[11px] text-text-muted truncate">
                      {agentId}
                    </div>
                  </div>
                  <button
                    onClick={() => {
                      const updated = (project.user_agents ?? []).filter((id) => id !== agentId);
                      setProject({ ...project, user_agents: updated.length > 0 ? updated : undefined });
                      setDirty(true);
                    }}
                    className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0"
                    title="Remove"
                  >
                    <X size={12} />
                  </button>
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
