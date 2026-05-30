// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useState } from "react";
import { Check, CheckCircle2, LayoutTemplate, Search, X } from "lucide-react";
import type { ProjectTemplate } from "../types";

export function ApplyProjectTemplateModal({
  templates,
  selected,
  onSelect,
  onCancel,
  onConfirm,
  result,
  onAcknowledge,
}: {
  templates: ProjectTemplate[];
  selected: string | null;
  onSelect: (name: string) => void;
  onCancel: () => void;
  onConfirm: () => void;
  result: {
    templateName: string;
    added: {
      agents: string[];
      skills: string[];
      mcp_servers: string[];
      user_agents: string[];
      user_commands: string[];
      hooks: string[];
      rules: string[];
    };
    hasUnifiedContent: boolean;
    saveRequired: boolean;
  } | null;
  onAcknowledge: () => void;
}) {
  const [filter, setFilter] = useState("");

  if (result) {
    return (
      <ApplyTemplateResultView result={result} onAcknowledge={onAcknowledge} />
    );
  }

  const trimmed = filter.trim().toLowerCase();
  const visible = trimmed
    ? templates.filter(
        (t) =>
          t.name.toLowerCase().includes(trimmed) ||
          (t.description ?? "").toLowerCase().includes(trimmed)
      )
    : templates;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onCancel} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-md mx-4 flex flex-col max-h-[80vh]">
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40 flex-shrink-0">
          <h2 className="text-[15px] font-semibold text-text-base">Apply Project Template</h2>
          <button
            onClick={onCancel}
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-5 pt-3 pb-2 flex-shrink-0">
          <p className="text-[12px] text-text-muted leading-relaxed mb-3">
            Select a template to apply to this project. Resources will only be added — existing
            project configuration will not be overwritten or removed.
          </p>
          {templates.length > 0 && (
            <div className="flex items-center gap-2 px-3 py-2 bg-bg-base border border-border-strong/40 rounded-md">
              <Search size={12} className="text-text-muted shrink-0" />
              <input
                type="text"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter templates..."
                autoFocus
                className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
              />
              {filter && (
                <button
                  onClick={() => setFilter("")}
                  className="text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={11} />
                </button>
              )}
            </div>
          )}
        </div>

        <div className="flex-1 overflow-y-auto custom-scrollbar px-3 pb-3 min-h-0">
          {templates.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              No project templates yet. Create one in the Project Templates section.
            </div>
          ) : visible.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              No templates match.
            </div>
          ) : (
            <ul className="space-y-1">
              {visible.map((t) => {
                const isSelected = selected === t.name;
                return (
                  <li key={t.name}>
                    <button
                      onClick={() => onSelect(t.name)}
                      onDoubleClick={() => { onSelect(t.name); onConfirm(); }}
                      className={`w-full flex items-start gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isSelected
                          ? "bg-brand/15 border border-brand/40"
                          : "border border-transparent hover:bg-bg-sidebar"
                      }`}
                    >
                      <LayoutTemplate
                        size={14}
                        className={`mt-0.5 flex-shrink-0 ${isSelected ? "text-brand" : "text-text-muted"}`}
                      />
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">{t.name}</div>
                        {t.description && (
                          <div className="text-[11px] text-text-muted truncate">{t.description}</div>
                        )}
                        <div className="flex items-center gap-2 mt-1">
                          {t.agents.length > 0 && (
                            <span className="text-[10px] text-text-muted">{t.agents.length} agents</span>
                          )}
                          {t.skills.length > 0 && (
                            <span className="text-[10px] text-text-muted">{t.skills.length} skills</span>
                          )}
                          {t.mcp_servers.length > 0 && (
                            <span className="text-[10px] text-text-muted">{t.mcp_servers.length} MCP</span>
                          )}
                        </div>
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong/40 flex-shrink-0">
          <button
            onClick={onCancel}
            className="flex h-[28px] items-center px-3 text-[12px] text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={!selected}
            className="flex h-[28px] items-center gap-1.5 px-3 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
          >
            <Check size={12} /> Apply
          </button>
        </div>
      </div>
    </div>
  );
}

export function ApplyTemplateResultView({
  result,
  onAcknowledge,
}: {
  result: {
    templateName: string;
    added: {
      agents: string[];
      skills: string[];
      mcp_servers: string[];
      user_agents: string[];
      user_commands: string[];
      hooks: string[];
      rules: string[];
    };
    hasUnifiedContent: boolean;
    saveRequired: boolean;
  };
  onAcknowledge: () => void;
}) {
  const sections: { label: string; items: string[] }[] = [
    { label: "Agents", items: result.added.agents },
    { label: "Skills", items: result.added.skills },
    { label: "MCP servers", items: result.added.mcp_servers },
    { label: "Sub-agents", items: result.added.user_agents },
    { label: "Commands", items: result.added.user_commands },
    { label: "Hooks", items: result.added.hooks },
    { label: "Rules", items: result.added.rules },
  ].filter((s) => s.items.length > 0);

  const totalAdded = sections.reduce((n, s) => n + s.items.length, 0);
  const nothingChanged = totalAdded === 0 && !result.hasUnifiedContent;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onAcknowledge} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-md mx-4 flex flex-col max-h-[80vh]">
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40 flex-shrink-0">
          <div className="flex items-center gap-2">
            <CheckCircle2 size={16} className="text-icon-skill" />
            <h2 className="text-[15px] font-semibold text-text-base">Template applied</h2>
          </div>
          <button
            onClick={onAcknowledge}
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto custom-scrollbar px-5 py-4 min-h-0">
          <p className="text-[13px] text-text-base mb-3">
            Applied template <span className="font-semibold">{result.templateName}</span> to this project.
          </p>

          {nothingChanged ? (
            <p className="text-[12px] text-text-muted leading-relaxed">
              No new resources were added — every item from this template was already present in
              this project.
            </p>
          ) : (
            <>
              {totalAdded > 0 ? (
                <div className="space-y-3">
                  {sections.map((section) => (
                    <div key={section.label}>
                      <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1.5">
                        {section.label} <span className="text-text-muted/70">({section.items.length})</span>
                      </div>
                      <div className="flex flex-wrap gap-1.5">
                        {section.items.map((item) => (
                          <span
                            key={item}
                            className="px-2 py-0.5 bg-bg-sidebar border border-border-strong/40 rounded text-[11px] text-text-base"
                          >
                            {item}
                          </span>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="text-[12px] text-text-muted leading-relaxed mb-3">
                  All resources from this template were already present in this project.
                </p>
              )}

              {result.hasUnifiedContent && result.saveRequired && (
                <div className="mt-4 px-3 py-2.5 bg-brand/10 border border-brand/30 rounded-md">
                  <p className="text-[12px] text-text-base leading-relaxed">
                    The template includes a unified instruction. Save the project to write it to disk.
                  </p>
                </div>
              )}
            </>
          )}
        </div>

        <div className="flex items-center justify-end px-5 py-3 border-t border-border-strong/40 flex-shrink-0">
          <button
            onClick={onAcknowledge}
            autoFocus
            className="flex h-[28px] items-center px-4 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors shadow-sm"
          >
            OK
          </button>
        </div>
      </div>
    </div>
  );
}

