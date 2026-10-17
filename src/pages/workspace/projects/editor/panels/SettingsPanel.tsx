// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { useState } from "react";
import { Copy, Layers, Lock } from "lucide-react";
import type { Project } from "../../types";
import { SettingSwitch } from "../SettingSwitch";

interface SettingsPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  /**
   * Turn read-only mode on or off on this machine. Takes effect at once,
   * outside the normal save, because a read-only project is never saved.
   */
  onReadOnlyChange: (readOnly: boolean) => Promise<void>;
  /**
   * The editor holds edits not yet saved. Turning read-only on would drop
   * them, so it waits until they are saved.
   */
  hasUnsavedChanges?: boolean;
}

export function SettingsPanel({
  project,
  setProject,
  setDirty,
  onReadOnlyChange,
  hasUnsavedChanges = false,
}: SettingsPanelProps) {
  const ownReadOnly = project.local_read_only === true;
  const inheritedFrom = project.read_only_owner;
  // Read-only here, on its own or through a read-only parent folder.
  const readOnly = ownReadOnly || Boolean(inheritedFrom);
  const hasFolder = Boolean(project.directory);
  // Turning it on needs a folder and no pending edits; turning it off is
  // always allowed. An inherited state is changed on the parent project.
  const toggleDisabled =
    Boolean(inheritedFrom) || (!ownReadOnly && (!hasFolder || hasUnsavedChanges));
  const [readOnlyBusy, setReadOnlyBusy] = useState(false);

  const toggleReadOnly = async () => {
    setReadOnlyBusy(true);
    try {
      await onReadOnlyChange(!ownReadOnly);
    } finally {
      setReadOnlyBusy(false);
    }
  };

  return (
    <section className="flex gap-6">
      <div className="flex-1 min-w-0 space-y-4">
        {/* Section header */}
        <div className="flex items-center gap-2">
          <Layers size={13} className="text-text-muted" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Settings</span>
        </div>

        {/* Read only (this machine) */}
        <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          <div className="px-4 py-3 flex items-center justify-between gap-4">
            <div className="min-w-0">
              <div className="flex items-center gap-1.5 text-[13px] font-medium text-text-base">
                <Lock size={12} className="text-text-muted" />
                Read only
              </div>
              <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
                Automatic reads this project&apos;s settings and still serves them to agents, but never
                changes any file in the project folder. This applies on this computer only.
                {inheritedFrom && (
                  <> This folder is inside &ldquo;{inheritedFrom}&rdquo;, which is read only, so this project is too.
                  Change it on &ldquo;{inheritedFrom}&rdquo;.</>
                )}
                {!readOnly && !hasFolder && " Choose a folder for this project first."}
                {!readOnly && hasFolder && hasUnsavedChanges && " Save your changes first."}
              </div>
            </div>
            <SettingSwitch
              checked={readOnly}
              disabled={readOnlyBusy || toggleDisabled}
              label="Read only on this computer"
              onToggle={() => void toggleReadOnly()}
            />
          </div>
        </div>

        {/* Read-only hint for new checkouts (committed) */}
        <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          <div className="px-4 py-3 flex items-center justify-between gap-4">
            <div className="min-w-0">
              <div className="text-[13px] font-medium text-text-base">Read only for new checkouts</div>
              <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
                Saved in{" "}
                <code className="bg-bg-sidebar px-1 rounded text-[10px]">.automatic.json</code>. Anyone
                who adds this project from a copy of the repository starts in read-only mode. It does not
                make this computer read only.
                {readOnly && " Turn off read only above to change it."}
              </div>
            </div>
            <SettingSwitch
              checked={project.read_only === true}
              disabled={readOnly}
              label="Read only for new checkouts"
              onToggle={() => {
                setProject({ ...project, read_only: !project.read_only, updated_at: new Date().toISOString() });
                setDirty(true);
              }}
            />
          </div>
        </div>

        {/* Sync Mode */}
        <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          <div className="px-4 py-3 border-b border-border-strong/30 flex items-center justify-between gap-4">
            <div className="min-w-0">
              <div className="text-[13px] font-medium text-text-base">Sync Mode</div>
              <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
                {project.mode === 'silent'
                  ? <>Files are written to <code className="bg-bg-sidebar px-1 rounded text-[10px]">.automatic/silent/</code> only — the project root is left untouched.</>
                  : "Files are written directly into the project directory (CLAUDE.md, .agents/, .claude/, etc.)."}
              </div>
            </div>
            <div className="flex gap-1.5 flex-shrink-0">
              <button
                disabled={readOnly}
                onClick={() => {
                  if (project.mode !== 'normal') {
                    setProject({ ...project, mode: 'normal', updated_at: new Date().toISOString() });
                    setDirty(true);
                  }
                }}
                className={`px-3 py-1.5 rounded text-[12px] font-medium border transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
                  (project.mode ?? 'normal') === 'normal'
                    ? 'bg-brand/10 border-brand/30 text-brand'
                    : 'bg-bg-sidebar border-border-strong/30 text-text-muted hover:text-text-base'
                }`}
              >
                Normal
              </button>
              <button
                disabled={readOnly}
                onClick={() => {
                  if (project.mode !== 'silent') {
                    setProject({ ...project, mode: 'silent', updated_at: new Date().toISOString() });
                    setDirty(true);
                  }
                }}
                className={`px-3 py-1.5 rounded text-[12px] font-medium border transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
                  project.mode === 'silent'
                    ? 'bg-brand/10 border-brand/30 text-brand'
                    : 'bg-bg-sidebar border-border-strong/30 text-text-muted hover:text-text-base'
                }`}
              >
                Silent
              </button>
            </div>
          </div>
          {project.mode === 'silent' && (
            <div className="px-4 py-3 bg-bg-sidebar/50 space-y-2">
              <p className="text-[11px] text-text-muted leading-relaxed">
                Silent mode is designed for existing codebases where Automatic&apos;s normal output files
                (instruction files, skills, agent configs) cannot be added to the project root.
                All synced content is mirrored under{" "}
                <code className="bg-bg-input px-1 rounded text-[10px]">.automatic/silent/</code>.
              </p>
              <p className="text-[11px] text-text-muted">
                Use the prompt below to tell an agent where to find Automatic&apos;s output:
              </p>
              <button
                onClick={() => {
                  const silentDir = `${project.directory}/.automatic/silent`;
                  const prompt = [
                    `This project uses Automatic in Silent mode.`,
                    `All Automatic-generated config is stored under \`.automatic/silent/\` rather than the project root.`,
                    ``,
                    `When looking for agent configuration, check:`,
                    `- Instruction files (CLAUDE.md, AGENTS.md, etc.): \`.automatic/silent/\``,
                    `- Skills: \`.automatic/silent/.agents/skills/\``,
                    `- Sub-agents: \`.automatic/silent/.claude/agents/\` (or equivalent for your agent)`,
                    `- MCP config: \`.automatic/silent/.claude/\` (or equivalent)`,
                    `- Commands: \`.automatic/silent/.agents/commands/\``,
                    ``,
                    `Full path: \`${silentDir}\``,
                  ].join('\n');
                  navigator.clipboard.writeText(prompt).catch(() => {});
                }}
                className="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-medium rounded-md border border-border-strong/40 bg-bg-input text-text-muted hover:text-text-base hover:border-border-strong/60 transition-colors"
              >
                <Copy size={11} />
                Copy agent prompt
              </button>
            </div>
          )}
        </div>

        {/* Manage .gitignore */}
        <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          <div className="px-4 py-3 flex items-center justify-between gap-4">
            <div className="min-w-0">
              <div className="text-[13px] font-medium text-text-base">Manage .gitignore</div>
              <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
                Keep a managed block in the project&apos;s{" "}
                <code className="bg-bg-sidebar px-1 rounded text-[10px]">.gitignore</code> listing every
                file Automatic writes (instruction files and agent config directories), so generated
                config is not committed. Turning this off removes the block on the next sync.
              </div>
            </div>
            <SettingSwitch
              checked={!!project.manage_gitignore}
              disabled={readOnly}
              label="Manage .gitignore for this project"
              onToggle={() => {
                setProject({ ...project, manage_gitignore: !project.manage_gitignore, updated_at: new Date().toISOString() });
                setDirty(true);
              }}
            />
          </div>
        </div>
      </div>

      {/* Help sidebar */}
      <div className="w-52 flex-shrink-0">
        <div className="rounded-md bg-bg-input border border-border-strong/30 px-3 py-2.5 text-[11px] text-text-muted space-y-1.5 sticky top-0">
          <p className="font-medium text-text-base text-[12px]">Read only</p>
          <p className="leading-relaxed">
            Use this for a repository someone else set up. Automatic loads its committed settings, and
            agents can still use the Automatic MCP service, but no file in the folder is changed.
          </p>
          <p className="font-medium text-text-base text-[12px] pt-1">Sync Mode</p>
          <p className="leading-relaxed">
            <strong className="text-text-base font-medium">Normal</strong> — the default. Automatic writes
            CLAUDE.md, .agents/, .claude/ and other config files directly into your project directory.
          </p>
          <p className="leading-relaxed">
            <strong className="text-text-base font-medium">Silent</strong> — for codebases where you
            can&apos;t add Automatic&apos;s files to the project root. All output is redirected to{" "}
            <code className="bg-bg-sidebar px-1 rounded text-[10px]">.automatic/silent/</code>.
            Copy the agent prompt to tell your agent where to look.
          </p>
        </div>
      </div>
    </section>
  );
}
