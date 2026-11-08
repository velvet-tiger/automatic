import type { ReactNode } from "react";
import { Copy, ExternalLink, Link as LinkIcon, Trash2 } from "lucide-react";
import { handleExternalLinkClick } from "../../../../../lib/externalLinks";
import type {
  CustomAssetConflict,
  DriftedFile,
  DriftReport,
  InstructionFileConflict,
  Project,
  ProjectNotice,
  ProjectProblem,
} from "../../types";
import {
  NOTICE_DANGER_BUTTON,
  NOTICE_PRIMARY_BUTTON,
  NOTICE_SECONDARY_BUTTON,
  NoticeChip,
  NoticeRow,
} from "./NoticeRow";
import { PendingProfilesNotice } from "./PendingProfilesNotice";
import { PendingContextsNotice } from "./PendingContextsNotice";
import { PendingRegistrationNotice } from "./PendingRegistrationNotice";

/**
 * Render backend problem text, where `backtick-wrapped` spans are commands or
 * paths, with those spans as inline code.
 */
export function renderInlineCode(text: string): ReactNode[] {
  return text.split(/(`[^`]+`)/).map((part, i) =>
    part.length > 2 && part.startsWith("`") && part.endsWith("`") ? (
      <code key={i} className="font-mono text-[10px] bg-bg-sidebar border border-border-strong/50 rounded px-1 py-px select-all">
        {part.slice(1, -1)}
      </code>
    ) : (
      part
    ),
  );
}

/** The prompt that tells an agent where Silent mode keeps the config. */
export function silentModePrompt(directory: string): string {
  return [
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
    `Full path: \`${directory}/.automatic/silent\``,
  ].join("\n");
}

/** What the editor lets a notice do. Each one is an existing editor action. */
export interface ProjectNoticeActions {
  project: Project;
  /** local_key of the open project. */
  selectedKey: string;
  readOnly: boolean;
  syncing: boolean;
  onSync: () => void;
  onRelinkFolder: () => void;
  /** The normal project removal, with its own confirmation. */
  onRemoveProject: () => Promise<void>;
  onOpenDriftFile: (file: DriftedFile, label: string) => void;
  onOpenInstructionConflict: (conflict: InstructionFileConflict) => void;
  onOpenCustomConflict: (conflict: CustomAssetConflict) => void;
  reloadProject: (projectKey: string) => Promise<void>;
}

interface ProjectNoticeViewProps extends ProjectNoticeActions {
  notice: ProjectNotice;
}

/** Draw one notice from the project notice list as one or more rows. */
export function ProjectNoticeView({ notice, ...actions }: ProjectNoticeViewProps) {
  const { source } = notice;
  switch (source.kind) {
    case "missing_directory":
      return (
        <NoticeRow
          severity="problem"
          title="Project folder not found"
          actions={
            <>
              <button onClick={actions.onRelinkFolder} className={NOTICE_PRIMARY_BUTTON}>
                <LinkIcon size={12} />
                Relink folder
              </button>
              <button onClick={() => void actions.onRemoveProject()} className={NOTICE_DANGER_BUTTON}>
                <Trash2 size={12} />
                Delete project
              </button>
            </>
          }
        >
          <div className="font-mono truncate">{source.directory}</div>
          <div className="mt-0.5">This folder has been moved or deleted. Relink it to continue syncing.</div>
        </NoticeRow>
      );
    case "check_failure":
      return (
        <NoticeRow severity="problem" title={source.title} alert>
          {source.error}
        </NoticeRow>
      );
    case "problem":
      return <ProblemRow problem={source.problem} />;
    case "drift":
      return <DriftRow report={source.report} actions={actions} />;
    case "pending_registration":
      return (
        <PendingRegistrationNotice
          projectName={actions.project.name}
          selectedKey={actions.selectedKey}
          reloadProject={actions.reloadProject}
          onRemove={actions.onRemoveProject}
        />
      );
    case "pending_profiles":
      return <PendingProfilesNotice project={actions.project} selectedKey={actions.selectedKey} reloadProject={actions.reloadProject} />;
    case "pending_contexts":
      return <PendingContextsNotice project={actions.project} selectedKey={actions.selectedKey} reloadProject={actions.reloadProject} />;
    case "silent_mode":
      return (
        <NoticeRow
          severity="notice"
          title="Silent mode is on"
          actions={
            <button
              title="Copy a prompt that tells agents where to find the config"
              onClick={() => {
                navigator.clipboard.writeText(silentModePrompt(source.directory)).catch((err: unknown) => {
                  console.warn("Could not copy the Silent mode prompt:", err);
                });
              }}
              className={NOTICE_SECONDARY_BUTTON}
            >
              <Copy size={11} />
              Copy prompt
            </button>
          }
        >
          Automatic writes all files to{" "}
          <code className="font-mono text-[10px] bg-bg-sidebar border border-border-strong/50 rounded px-1">.automatic/silent/</code>{" "}
          instead of the project root, leaving existing files untouched.
        </NoticeRow>
      );
    case "sync_notes":
      return (
        <NoticeRow severity="notice" title="Sync notes">
          <ul className="mt-0.5 space-y-0.5 list-disc pl-4">
            {source.notes.map((note, j) => (
              <li key={`${j}-${note}`}>{renderInlineCode(note)}</li>
            ))}
          </ul>
        </NoticeRow>
      );
  }
}

function ProblemRow({ problem }: { problem: ProjectProblem }) {
  return (
    <NoticeRow
      severity="problem"
      title={problem.title}
      actions={
        problem.reference_url && (
          <a
            href={problem.reference_url}
            onClick={handleExternalLinkClick(problem.reference_url)}
            className="flex items-center gap-1 text-[11px] text-text-muted hover:text-text-base underline decoration-border-strong transition-colors"
          >
            <ExternalLink size={10} />
            Docs
          </a>
        )
      }
    >
      <div>{renderInlineCode(problem.description)}</div>
      {problem.solution && problem.solution.length > 0 && (
        <div className="mt-1.5">
          <div className="font-semibold text-text-base">How to fix</div>
          <ul className="mt-0.5 space-y-0.5 list-disc pl-4">
            {problem.solution.map((step, j) => (
              <li key={`${j}-${step}`}>{renderInlineCode(step)}</li>
            ))}
          </ul>
        </div>
      )}
      {problem.resources.length > 0 && (
        <div className="flex flex-wrap gap-1 mt-1.5">
          {problem.resources.map((r) => (
            <NoticeChip key={r}>{r}</NoticeChip>
          ))}
        </div>
      )}
    </NoticeRow>
  );
}

function DriftGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="mt-1.5">
      <div className="text-[11px] font-semibold text-text-base mb-0.5">{label}</div>
      <div className="flex flex-wrap gap-x-2 gap-y-1">{children}</div>
    </div>
  );
}

/**
 * Drift describes real files on disk. A read-only project still shows it, as
 * information: the folder's agent files differ from its settings, which can
 * mean committed files Automatic does not manage. It cannot be synced.
 */
function DriftRow({ report, actions }: { report: DriftReport; actions: ProjectNoticeActions }) {
  const instructionConflicts = report.instruction_conflicts ?? [];
  const customConflicts = report.custom_conflicts ?? [];
  const gitignore = report.gitignore;
  return (
    <NoticeRow
      severity="warning"
      title={actions.readOnly ? "Files differ from the Automatic settings" : "Configuration has drifted"}
      actions={
        !actions.readOnly && (
          <button onClick={actions.onSync} disabled={actions.syncing} className={NOTICE_PRIMARY_BUTTON}>
            {actions.syncing ? "Syncing…" : "Sync now"}
          </button>
        )
      }
    >
      <div>
        {actions.readOnly
          ? "Files Automatic manages in this folder differ from its settings. Read-only mode leaves them as they are."
          : "Files Automatic manages no longer match its settings. Click a file to see the change."}
      </div>
      {report.agents.map((agentDrift) => (
        <DriftGroup key={agentDrift.agent_id} label={agentDrift.agent_label}>
          {agentDrift.files.map((f, i) => (
            <NoticeChip key={i} title="View diff" onClick={() => actions.onOpenDriftFile(f, agentDrift.agent_label)}>
              {f.path}
              <span className="font-sans ml-0.5 opacity-70">({f.reason})</span>
            </NoticeChip>
          ))}
        </DriftGroup>
      ))}
      {/* It belongs to the project, not to one agent, and only the block is
          compared, never the whole file. */}
      {gitignore && (
        <DriftGroup label="Managed .gitignore block">
          <NoticeChip title="View diff" onClick={() => actions.onOpenDriftFile(gitignore, "Managed block")}>
            {gitignore.path}
            <span className="font-sans ml-0.5 opacity-70">({gitignore.reason})</span>
          </NoticeChip>
        </DriftGroup>
      )}
      {instructionConflicts.length > 0 && (
        <DriftGroup label="Instruction files">
          {instructionConflicts.map((c) => (
            <NoticeChip key={c.filename} title="Resolve conflict" onClick={() => actions.onOpenInstructionConflict(c)}>
              {c.filename}
              <span className="font-sans ml-0.5 opacity-70">(conflict)</span>
            </NoticeChip>
          ))}
        </DriftGroup>
      )}
      {customConflicts.length > 0 && (
        <DriftGroup label="Project custom assets">
          {customConflicts.map((c) => (
            <NoticeChip key={`${c.kind}:${c.name}`} title="Resolve conflict" onClick={() => actions.onOpenCustomConflict(c)}>
              {c.name}
              <span className="font-sans ml-0.5 opacity-70">({c.kind})</span>
            </NoticeChip>
          ))}
        </DriftGroup>
      )}
    </NoticeRow>
  );
}
