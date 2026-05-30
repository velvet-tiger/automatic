// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { Copy, Layers } from "lucide-react";
import type { Project } from "../../types";

interface SettingsPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
}

export function SettingsPanel({ project, setProject, setDirty }: SettingsPanelProps) {
  return (
    <section className="flex gap-6">
      <div className="flex-1 min-w-0 space-y-4">
        {/* Section header */}
        <div className="flex items-center gap-2">
          <Layers size={13} className="text-text-muted" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Settings</span>
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
                onClick={() => {
                  if (project.mode !== 'normal') {
                    setProject({ ...project, mode: 'normal', updated_at: new Date().toISOString() });
                    setDirty(true);
                  }
                }}
                className={`px-3 py-1.5 rounded text-[12px] font-medium border transition-colors ${
                  (project.mode ?? 'normal') === 'normal'
                    ? 'bg-brand/10 border-brand/30 text-brand'
                    : 'bg-bg-sidebar border-border-strong/30 text-text-muted hover:text-text-base'
                }`}
              >
                Normal
              </button>
              <button
                onClick={() => {
                  if (project.mode !== 'silent') {
                    setProject({ ...project, mode: 'silent', updated_at: new Date().toISOString() });
                    setDirty(true);
                  }
                }}
                className={`px-3 py-1.5 rounded text-[12px] font-medium border transition-colors ${
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
      </div>

      {/* Help sidebar */}
      <div className="w-52 flex-shrink-0">
        <div className="rounded-md bg-bg-input border border-border-strong/30 px-3 py-2.5 text-[11px] text-text-muted space-y-1.5 sticky top-0">
          <p className="font-medium text-text-base text-[12px]">Sync Mode</p>
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
