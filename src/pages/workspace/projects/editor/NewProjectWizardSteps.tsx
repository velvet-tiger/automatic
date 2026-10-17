// Optional steps of the new-project wizard: group, profile, tools and settings.
// The steps only collect choices. ProjectEditor applies them when the project is created.

import type { ReactNode } from "react";
import { ArrowRight, Check, Layers, Settings, UserCog, Wrench } from "lucide-react";
import type { Project, ProjectProfile, ProjectToolEntry } from "../types";
import { SettingSwitch } from "./SettingSwitch";

/** 1 Directory, 2 Agents, 3 Group, 4 Profile, 5 Templates, 6 Tools, 7 Settings. */
export const WIZARD_STEPS = [1, 2, 3, 4, 5, 6, 7] as const;
export type WizardStepNumber = (typeof WIZARD_STEPS)[number];

/** Short names for the step indicator's tooltips. */
export const WIZARD_STEP_LABELS: Record<WizardStepNumber, string> = {
  1: "Directory",
  2: "Agents",
  3: "Group",
  4: "Profile",
  5: "Templates",
  6: "Tools",
  7: "Settings",
};

/** The settings the wizard can set before the project exists. */
export type WizardSettings = Pick<Project, "mode" | "manage_gitignore" | "read_only">;

export function WizardStepFrame({
  icon,
  title,
  description,
  children,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <>
      <div className="mb-6 text-center">
        <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-brand/10 border border-brand/30 flex items-center justify-center">
          {icon}
        </div>
        <h2 className="text-[16px] font-semibold text-text-base mb-1">{title}</h2>
        <p className="text-[13px] text-text-muted leading-relaxed">{description}</p>
      </div>
      {children}
    </>
  );
}

/**
 * Back and Continue buttons. An optional step with nothing chosen reads
 * "Skip", so the user can see that leaving it empty is fine.
 */
export function WizardNav({
  onBack,
  onNext,
  nextLabel,
  nextIcon,
}: {
  onBack: () => void;
  onNext: () => void;
  nextLabel: string;
  nextIcon?: ReactNode;
}) {
  return (
    <div className="flex gap-2 mt-6">
      <button
        onClick={onBack}
        className="flex-1 px-4 py-2.5 bg-bg-sidebar hover:bg-surface text-text-muted hover:text-text-base text-[13px] font-medium rounded transition-colors"
      >
        Back
      </button>
      <button
        onClick={onNext}
        className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
      >
        {nextIcon ?? <ArrowRight size={13} />} {nextLabel}
      </button>
    </div>
  );
}

/** "Skip" when nothing is chosen, otherwise "Continue". */
export function continueLabel(selectedCount: number): string {
  return selectedCount === 0 ? "Skip" : "Continue";
}

function WizardChoice({
  selected,
  onToggle,
  title,
  description,
  badge,
  locked = false,
}: {
  selected: boolean;
  onToggle: () => void;
  title: string;
  description?: string;
  badge?: string;
  /** A locked choice cannot be changed here. Something else owns it. */
  locked?: boolean;
}) {
  return (
    <button
      role="checkbox"
      aria-checked={selected}
      disabled={locked}
      onClick={onToggle}
      className={`w-full text-left px-3 py-2.5 rounded-md transition-colors flex items-start gap-2 border disabled:cursor-not-allowed ${
        selected
          ? "bg-brand/15 border-brand/40"
          : "bg-bg-input border-border-strong/40 hover:border-border-strong hover:bg-bg-sidebar"
      }`}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-[13px] font-medium text-text-base">{title}</span>
          {badge && (
            <span className="inline-flex items-center px-1.5 py-0.5 rounded-full bg-brand/10 border border-brand/20 text-[10px] font-medium text-brand">
              {badge}
            </span>
          )}
        </div>
        {description && <div className="text-[11px] text-text-muted mt-0.5 truncate">{description}</div>}
      </div>
      {selected && <Check size={13} className="text-brand flex-shrink-0 mt-0.5" />}
    </button>
  );
}

function EmptyNote({ children }: { children: ReactNode }) {
  return (
    <div className="px-3 py-4 bg-bg-input border border-border-strong/40 rounded-md text-center">
      <p className="text-[12px] text-text-muted italic">{children}</p>
    </div>
  );
}

function toggled(list: string[], name: string): string[] {
  return list.includes(name) ? list.filter((n) => n !== name) : [...list, name];
}

export function WizardGroupStep({
  groups,
  loading,
  selected,
  onChange,
  onBack,
  onNext,
}: {
  groups: string[];
  loading: boolean;
  selected: string[];
  onChange: (next: string[]) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  return (
    <WizardStepFrame
      icon={<Layers size={24} className="text-brand" strokeWidth={1.5} />}
      title="Add to a group"
      description="Optionally add this project to one or more groups. Agents see the other projects in each group."
    >
      {loading ? (
        <EmptyNote>Loading groups…</EmptyNote>
      ) : groups.length === 0 ? (
        <EmptyNote>No groups exist yet. You can create one later and add this project to it.</EmptyNote>
      ) : (
        <div className="space-y-1 max-h-56 overflow-y-auto custom-scrollbar">
          {groups.map((name) => (
            <WizardChoice
              key={name}
              title={name}
              selected={selected.includes(name)}
              onToggle={() => onChange(toggled(selected, name))}
            />
          ))}
        </div>
      )}
      <WizardNav onBack={onBack} onNext={onNext} nextLabel={continueLabel(selected.length)} />
    </WizardStepFrame>
  );
}

/**
 * Which selected group provides each profile, keyed by profile name. A
 * profile that several selected groups list is credited to the first group
 * by name, as the backend does. `groupProfiles` maps a group name to the
 * profiles it lists. A selected group missing from the map provides nothing.
 */
export function profilesProvidedByGroups(
  selectedGroups: string[],
  groupProfiles: Record<string, string[]>
): Record<string, string> {
  const provided: Record<string, string> = {};
  for (const group of [...selectedGroups].sort((a, b) => a.localeCompare(b))) {
    for (const profile of groupProfiles[group] ?? []) {
      if (!(profile in provided)) provided[profile] = group;
    }
  }
  return provided;
}

export function WizardProfileStep({
  profiles,
  providedByGroup,
  selected,
  onChange,
  onBack,
  onNext,
}: {
  profiles: ProjectProfile[];
  /** Profiles a selected group gives the project, keyed by profile name. They cannot be unticked here. */
  providedByGroup: Record<string, string>;
  selected: string[];
  onChange: (next: string[]) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  // A provided profile missing from the library list still gets a row, so the user sees it.
  const known = new Set(profiles.map((p) => p.name));
  const orphanProvided = Object.keys(providedByGroup).filter((name) => !known.has(name));
  const chosenCount = selected.filter((name) => !(name in providedByGroup)).length;
  const hasRows = profiles.length > 0 || orphanProvided.length > 0;
  return (
    <WizardStepFrame
      icon={<UserCog size={24} className="text-brand" strokeWidth={1.5} />}
      title="Assign a profile"
      description="Optionally attach one or more profiles. A profile keeps its skills, MCP servers and rules in step across every project that uses it."
    >
      {!hasRows ? (
        <EmptyNote>No profiles yet. You can create one in the Library and attach it later.</EmptyNote>
      ) : (
        <div className="space-y-1 max-h-56 overflow-y-auto custom-scrollbar">
          {[...profiles.map((p) => ({ name: p.name, description: p.description })),
            ...orphanProvided.map((name) => ({ name, description: "" }))].map((p) => {
            const group = providedByGroup[p.name];
            return (
              <WizardChoice
                key={p.name}
                title={p.name}
                description={p.description}
                badge={group ? `From group: ${group}` : undefined}
                locked={group !== undefined}
                selected={group !== undefined || selected.includes(p.name)}
                onToggle={() => onChange(toggled(selected, p.name))}
              />
            );
          })}
        </div>
      )}
      <WizardNav
        onBack={onBack}
        onNext={onNext}
        nextLabel={continueLabel(chosenCount + Object.keys(providedByGroup).length)}
      />
    </WizardStepFrame>
  );
}

export function WizardToolsStep({
  entries,
  loading,
  detected,
  selected,
  onChange,
  onBack,
  onNext,
}: {
  entries: ProjectToolEntry[];
  loading: boolean;
  /** Names of tools found in the project directory. */
  detected: string[];
  selected: string[];
  onChange: (next: string[]) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  // Machine-wide tools have no per-project effect, so the Tools tab leaves them out too.
  const projectScoped = entries.filter((e) => e.project_scoped);
  return (
    <WizardStepFrame
      icon={<Wrench size={24} className="text-brand" strokeWidth={1.5} />}
      title="Enable tools"
      description="Optionally turn on the tools that your enabled plugins provide for this project."
    >
      {loading ? (
        <EmptyNote>Loading tools…</EmptyNote>
      ) : projectScoped.length === 0 ? (
        <EmptyNote>No tools available. Enable a plugin in Settings to register tools.</EmptyNote>
      ) : (
        <div className="space-y-1 max-h-56 overflow-y-auto custom-scrollbar">
          {projectScoped.map((e) => (
            <WizardChoice
              key={e.name}
              title={e.display_name}
              description={e.description}
              badge={detected.includes(e.name) ? "Detected" : undefined}
              selected={selected.includes(e.name)}
              onToggle={() => onChange(toggled(selected, e.name))}
            />
          ))}
        </div>
      )}
      <WizardNav onBack={onBack} onNext={onNext} nextLabel={continueLabel(selected.length)} />
    </WizardStepFrame>
  );
}

const MODE_BUTTON_BASE =
  "px-3 py-1.5 rounded text-[12px] font-medium border transition-colors";
const MODE_BUTTON_ON = "bg-brand/10 border-brand/30 text-brand";
const MODE_BUTTON_OFF =
  "bg-bg-sidebar border-border-strong/30 text-text-muted hover:text-text-base";

export function WizardSettingsStep({
  settings,
  onChange,
  onBack,
  onCreate,
}: {
  settings: WizardSettings;
  onChange: (patch: Partial<WizardSettings>) => void;
  onBack: () => void;
  onCreate: () => void;
}) {
  const silent = settings.mode === "silent";
  return (
    <WizardStepFrame
      icon={<Settings size={24} className="text-brand" strokeWidth={1.5} />}
      title="Project settings"
      description="Choose how Automatic writes files for this project. You can change these later in the Settings tab."
    >
      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/30">
        <div className="px-4 py-3 flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="text-[13px] font-medium text-text-base">Sync Mode</div>
            <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
              {silent
                ? "Files are written to .automatic/silent/ only. The project root is left untouched."
                : "Files are written directly into the project directory (CLAUDE.md, .agents/, .claude/, etc.)."}
            </div>
          </div>
          <div className="flex gap-1.5 flex-shrink-0">
            <button
              onClick={() => onChange({ mode: "normal" })}
              className={`${MODE_BUTTON_BASE} ${!silent ? MODE_BUTTON_ON : MODE_BUTTON_OFF}`}
            >
              Normal
            </button>
            <button
              onClick={() => onChange({ mode: "silent" })}
              className={`${MODE_BUTTON_BASE} ${silent ? MODE_BUTTON_ON : MODE_BUTTON_OFF}`}
            >
              Silent
            </button>
          </div>
        </div>

        <div className="px-4 py-3 flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="text-[13px] font-medium text-text-base">Manage .gitignore</div>
            <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
              Keep a managed block in .gitignore listing every file Automatic writes, so generated
              config is not committed.
            </div>
          </div>
          <SettingSwitch
            checked={settings.manage_gitignore === true}
            label="Manage .gitignore for this project"
            onToggle={() => onChange({ manage_gitignore: !settings.manage_gitignore })}
          />
        </div>

        <div className="px-4 py-3 flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="text-[13px] font-medium text-text-base">Read only for new checkouts</div>
            <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
              Saved in .automatic.json. Anyone who adds this project from a copy of the repository
              starts in read-only mode. It does not make this computer read only.
            </div>
          </div>
          <SettingSwitch
            checked={settings.read_only === true}
            label="Read only for new checkouts"
            onToggle={() => onChange({ read_only: !settings.read_only })}
          />
        </div>
      </div>
      <WizardNav
        onBack={onBack}
        onNext={onCreate}
        nextLabel="Create Project"
        nextIcon={<Check size={13} />}
      />
    </WizardStepFrame>
  );
}
