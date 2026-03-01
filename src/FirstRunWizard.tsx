import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { identifyOnboarding } from "./analytics";
import { ChevronRight, Check, FolderOpen } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import graphLogo from "../logos/graph_5.svg";

// ── Types ─────────────────────────────────────────────────────────────────────

interface WizardAnswers {
  role: string;
  aiUsage: string;
  agents: string[];
  email: string;
  analyticsEnabled: boolean;
  createdProjectName?: string;
}

interface BundledTemplate {
  name: string;
  display_name: string;
  description: string;
  category: string;
  icon?: string;
  tags: string[];
  skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  unified_instruction?: string;
  unified_rules?: string[];
  project_files?: { filename: string; content: string }[];
}

interface FirstRunWizardProps {
  /** Called when the user completes (or skips) the wizard. */
  onComplete: (answers: WizardAnswers) => void;
}

// ── Option data ───────────────────────────────────────────────────────────────

const ROLES = [
  {
    id: "fullstack",
    label: "Full-stack developer",
    description: "Frontend and backend across the whole product",
  },
  {
    id: "frontend",
    label: "Frontend developer",
    description: "UI, design systems, and browser-side code",
  },
  {
    id: "backend",
    label: "Backend developer",
    description: "APIs, services, databases, and infrastructure",
  },
  {
    id: "mobile",
    label: "Mobile developer",
    description: "iOS, Android, or cross-platform apps",
  },
  {
    id: "devops",
    label: "DevOps / Platform engineer",
    description: "CI/CD, infrastructure, and developer tooling",
  },
  {
    id: "ml",
    label: "ML / AI engineer",
    description: "Models, pipelines, and data systems",
  },
  {
    id: "other",
    label: "Something else",
    description: "A role not listed above",
  },
];

const AI_USAGE_OPTIONS = [
  {
    id: "full_agentic",
    label: "Fully agentic",
    description:
      "Agents plan, write, and iterate on code with minimal hand-holding",
  },
  {
    id: "assisted",
    label: "AI-assisted",
    description: "I use AI for suggestions, completions, and code review",
  },
  {
    id: "occasional",
    label: "Occasional use",
    description: "I reach for AI when stuck or for one-off tasks",
  },
  {
    id: "experimenting",
    label: "Still experimenting",
    description: "I'm working out where AI fits in my workflow",
  },
  {
    id: "none",
    label: "Not yet",
    description: "I'm evaluating AI tooling but haven't adopted it",
  },
];

const AGENT_OPTIONS = [
  { id: "antigravity", label: "Antigravity" },
  { id: "claude", label: "Claude Code" },
  { id: "cline", label: "Cline" },
  { id: "codex_cli", label: "Codex CLI" },
  { id: "cursor", label: "Cursor" },
  { id: "droid", label: "Droid" },
  { id: "gemini_cli", label: "Gemini CLI" },
  { id: "github_copilot", label: "GitHub Copilot" },
  { id: "goose", label: "Goose" },
  { id: "junie", label: "Junie" },
  { id: "kilo_code", label: "Kilo Code" },
  { id: "kiro", label: "Kiro" },
  { id: "opencode", label: "OpenCode" },
  { id: "warp", label: "Warp" },
  { id: "other", label: "Other" },
];

// ── Step indicator ────────────────────────────────────────────────────────────

const STEPS = ["Your role", "AI workflow", "Your agents", "Stay in touch", "Preferences", "First project"];

function StepIndicator({ current }: { current: number }) {
  return (
    <div className="flex items-center mb-10">
      {STEPS.map((label, i) => {
        const done = i < current;
        const active = i === current;
        return (
          <React.Fragment key={i}>
            {/* Step: circle + label */}
            <div className="flex items-center gap-1.5 shrink-0">
              <div
                className={`w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-semibold transition-colors ${
                  done
                    ? "bg-brand text-white"
                    : active
                      ? "border-2 border-brand text-brand"
                      : "border border-border-strong/40 text-text-muted"
                }`}
              >
                {done ? <Check size={10} strokeWidth={3} /> : i + 1}
              </div>
              <span
                className={`text-[12px] whitespace-nowrap hidden sm:inline ${
                  active
                    ? "text-text-base font-medium"
                    : done
                      ? "text-brand"
                      : "text-text-muted"
                }`}
              >
                {label}
              </span>
            </div>
            {/* Connector line between steps */}
            {i < STEPS.length - 1 && (
              <div
                className={`flex-1 h-px mx-3 ${done ? "bg-brand" : "bg-surface-active"}`}
              />
            )}
          </React.Fragment>
        );
      })}
    </div>
  );
}

// ── Single-select option card ─────────────────────────────────────────────────

function OptionCard({
  label,
  description,
  selected,
  onClick,
}: {
  label: string;
  description?: string;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`w-full flex items-start gap-3 px-4 py-3 rounded-lg border text-left transition-all ${
        selected
          ? "border-brand bg-brand/10"
          : "border-border-strong/40 bg-bg-input-dark hover:border-border-strong hover:bg-surface-hover"
      }`}
    >
      <div
        className={`mt-0.5 w-4 h-4 rounded-full border-2 flex-shrink-0 flex items-center justify-center transition-colors ${
          selected ? "border-brand bg-brand" : "border-text-muted"
        }`}
      >
        {selected && <div className="w-1.5 h-1.5 rounded-full bg-white" />}
      </div>
      <div className="min-w-0">
        <div className="text-[13px] font-medium text-text-base">{label}</div>
        {description && (
          <div className="text-[12px] text-text-muted mt-0.5 leading-relaxed">
            {description}
          </div>
        )}
      </div>
    </button>
  );
}

// ── Multi-select chip ─────────────────────────────────────────────────────────

function AgentChip({
  label,
  selected,
  onClick,
}: {
  label: string;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-2 px-3 py-2 rounded-lg border text-[13px] font-medium transition-all ${
        selected
          ? "border-brand bg-brand/15 text-brand"
          : "border-border-strong/40 bg-bg-input-dark text-text-muted hover:border-border-strong hover:text-text-base"
      }`}
    >
      {selected && <Check size={12} className="text-brand" strokeWidth={3} />}
      {label}
    </button>
  );
}

// ── Toggle ────────────────────────────────────────────────────────────────────

function Toggle({
  enabled,
  onToggle,
}: {
  enabled: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      onClick={onToggle}
      className={`relative flex-shrink-0 w-10 h-5 rounded-full transition-colors focus:outline-none ${
        enabled ? "bg-brand" : "bg-surface-active"
      }`}
      role="switch"
      aria-checked={enabled}
    >
      <div
        className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-all ${
          enabled ? "left-5" : "left-0.5"
        }`}
      />
    </button>
  );
}

// ── Step screens ──────────────────────────────────────────────────────────────

function StepRole({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <h2 className="text-[22px] font-semibold text-text-base mb-1">
        What's your primary role?
      </h2>
      <p className="text-[14px] text-text-muted mb-6 leading-relaxed">
        This helps us surface the most relevant features and defaults for you.
      </p>
      <div className="space-y-2">
        {ROLES.map((r) => (
          <OptionCard
            key={r.id}
            label={r.label}
            description={r.description}
            selected={value === r.id}
            onClick={() => onChange(r.id)}
          />
        ))}
      </div>
    </div>
  );
}

function StepAiUsage({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <h2 className="text-[22px] font-semibold text-text-base mb-1">
        How do you use AI in your development process?
      </h2>
      <p className="text-[14px] text-text-muted mb-6 leading-relaxed">
        We'll use this to tailor how Automatic presents workflows and
        integrations.
      </p>
      <div className="space-y-2">
        {AI_USAGE_OPTIONS.map((o) => (
          <OptionCard
            key={o.id}
            label={o.label}
            description={o.description}
            selected={value === o.id}
            onClick={() => onChange(o.id)}
          />
        ))}
      </div>
    </div>
  );
}

function StepAgents({
  value,
  onChange,
}: {
  value: string[];
  onChange: (v: string[]) => void;
}) {
  const toggle = (id: string) => {
    onChange(
      value.includes(id) ? value.filter((a) => a !== id) : [...value, id]
    );
  };

  return (
    <div>
      <h2 className="text-[22px] font-semibold text-text-base mb-1">
        Which AI agents do you work with?
      </h2>
      <p className="text-[14px] text-text-muted mb-6 leading-relaxed">
        Select all that apply. Automatic can sync skills and MCP server configs
        to each of these tools.
      </p>
      <div className="flex flex-wrap gap-2">
        {AGENT_OPTIONS.map((a) => (
          <AgentChip
            key={a.id}
            label={a.label}
            selected={value.includes(a.id)}
            onClick={() => toggle(a.id)}
          />
        ))}
      </div>
      {value.length === 0 && (
        <p className="mt-4 text-[12px] text-text-muted italic">
          Select at least one agent to continue, or skip this step.
        </p>
      )}
    </div>
  );
}

function StepEmail({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <h2 className="text-[22px] font-semibold text-text-base mb-1">
        Stay in the loop
      </h2>
      <p className="text-[14px] text-text-muted mb-6 leading-relaxed">
        Get occasional updates on new features and improvements to Automatic.
        We won't spam you — unsubscribe any time.
      </p>
      <div className="space-y-2">
        <label className="block text-[12px] font-medium text-text-muted">
          Email address <span className="text-text-muted">(optional)</span>
        </label>
        <input
          type="email"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder="you@example.com"
          className="w-full px-3 py-2.5 rounded-lg border border-border-strong/40 bg-bg-input-dark text-[13px] text-text-base placeholder-text-muted focus:outline-none focus:border-brand focus:ring-1 focus:ring-brand/40 transition-colors"
        />
      </div>
    </div>
  );
}

function StepPreferences({
  analyticsEnabled,
  onToggleAnalytics,
}: {
  analyticsEnabled: boolean;
  onToggleAnalytics: () => void;
}) {
  return (
    <div>
      <h2 className="text-[22px] font-semibold text-text-base mb-1">
        A couple of quick preferences
      </h2>
      <p className="text-[14px] text-text-muted mb-6 leading-relaxed">
        You can change these at any time in Settings.
      </p>

      <div className="space-y-3">
        <button
          onClick={onToggleAnalytics}
          className={`flex items-center justify-between w-full p-4 rounded-lg border text-left transition-all ${
            analyticsEnabled
              ? "border-brand bg-brand/10"
              : "border-border-strong/40 bg-bg-input-dark hover:border-border-strong hover:bg-surface-hover"
          }`}
        >
          <div>
            <div className="text-[13px] font-medium text-text-base">
              Share anonymous usage data
            </div>
            <div className="text-[12px] text-text-muted mt-0.5 leading-relaxed">
              Helps us understand which features matter most. No personal data,
              file contents, or project names are ever collected.
            </div>
          </div>
          <div className="ml-4">
            <Toggle enabled={analyticsEnabled} onToggle={onToggleAnalytics} />
          </div>
        </button>
      </div>
    </div>
  );
}

// ── First project step ────────────────────────────────────────────────────────

function StepFirstProject({
  templates,
  selectedTemplate,
  projectDir,
  onSelectTemplate,
  onBrowse,
}: {
  templates: BundledTemplate[];
  selectedTemplate: string | null;
  projectDir: string;
  onSelectTemplate: (name: string | null) => void;
  onBrowse: () => void;
}) {
  return (
    <div>
      <h2 className="text-[22px] font-semibold text-text-base mb-1">
        Create your first project
      </h2>
      <p className="text-[14px] text-text-muted mb-6 leading-relaxed">
        Point Automatic at a project directory and it will detect your agents
        and tools. Optionally pick a template for pre-configured skills and
        instructions.
      </p>

      {/* Directory picker — always visible */}
      <div className="mb-6">
        <label className="block text-[12px] font-medium text-text-muted mb-2">
          Project directory
        </label>
        <div className="flex gap-2">
          <div
            onClick={onBrowse}
            className="flex-1 flex items-center px-3 py-2.5 rounded-lg border border-border-strong/40 bg-bg-input-dark text-[13px] cursor-pointer hover:border-border-strong transition-colors min-w-0"
          >
            {projectDir ? (
              <span className="text-text-base truncate">{projectDir}</span>
            ) : (
              <span className="text-text-muted">Select a folder...</span>
            )}
          </div>
          <button
            onClick={onBrowse}
            className="flex items-center gap-1.5 px-3 py-2.5 rounded-lg border border-border-strong/40 bg-bg-input-dark text-[13px] text-text-muted hover:border-border-strong hover:text-text-base transition-colors flex-shrink-0"
          >
            <FolderOpen size={14} />
            Browse
          </button>
        </div>
      </div>

      {/* Template grid — optional */}
      <div>
        <label className="block text-[12px] font-medium text-text-muted mb-2">
          Start from a template{" "}
          <span className="font-normal text-text-muted">(optional)</span>
        </label>
        <div className="grid grid-cols-2 gap-2">
          {templates.map((t) => {
            const isSelected = selectedTemplate === t.name;
            return (
              <button
                key={t.name}
                onClick={() => onSelectTemplate(isSelected ? null : t.name)}
                className={`text-left px-3 py-2.5 rounded-lg border transition-all ${
                  isSelected
                    ? "border-brand bg-brand/10"
                    : "border-border-strong/40 bg-bg-input-dark hover:border-border-strong hover:bg-surface-hover"
                }`}
              >
                <div className="text-[13px] font-medium text-text-base">
                  {t.display_name}
                </div>
                <div className="text-[11px] text-text-muted mt-0.5">
                  {t.category}
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ── Wizard shell ──────────────────────────────────────────────────────────────

export default function FirstRunWizard({ onComplete }: FirstRunWizardProps) {
  const [step, setStep] = useState(0);
  const [answers, setAnswers] = useState<WizardAnswers>({
    role: "",
    aiUsage: "",
    agents: [],
    email: "",
    analyticsEnabled: true,
  });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [subscribeError, setSubscribeError] = useState<string | null>(null);
  const [templates, setTemplates] = useState<BundledTemplate[]>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string | null>(null);
  const [projectDir, setProjectDir] = useState("");

  // Pre-populate answers from any previously saved onboarding data.
  useEffect(() => {
    async function loadSaved() {
      try {
        const settings: any = await invoke("read_settings");
        setAnswers({
          role: settings?.onboarding?.role ?? "",
          aiUsage: settings?.onboarding?.ai_usage ?? "",
          agents: settings?.onboarding?.agents ?? [],
          email: settings?.onboarding?.email ?? "",
          analyticsEnabled: settings?.analytics_enabled ?? true,
        });
      } catch (e) {
        console.error("[wizard] Failed to load saved settings:", e);
      } finally {
        setLoading(false);
      }
    }
    loadSaved();
  }, []);

  // Load bundled project templates for the first-project step.
  useEffect(() => {
    async function loadTemplates() {
      try {
        const raw: string = await invoke("list_bundled_project_templates");
        setTemplates(JSON.parse(raw));
      } catch (e) {
        console.error("[wizard] Failed to load bundled templates:", e);
      }
    }
    loadTemplates();
  }, []);

  const totalSteps = STEPS.length;
  const isLast = step === totalSteps - 1;

  const canAdvance = () => {
    if (step === 0) return answers.role !== "";
    if (step === 1) return answers.aiUsage !== "";
    // First project step: need a directory when a template is selected.
    if (step === 5) return !selectedTemplate || !!projectDir;
    // agents step: allow skipping (zero selection)
    return true;
  };

  const handleNext = async () => {
    if (isLast) {
      await finish();
    } else {
      setStep((s) => s + 1);
    }
  };

  const finish = async (skipProject?: boolean) => {
    setSaving(true);
    let createdProjectName: string | undefined;
    try {
      // Read current settings, patch wizard fields, write back.
      const current: any = await invoke("read_settings");
      const updated = {
        ...current,
        wizard_completed: true,
        analytics_enabled: answers.analyticsEnabled,
        onboarding: {
          role: answers.role,
          ai_usage: answers.aiUsage,
          agents: answers.agents,
          email: answers.email,
        },
      };
      await invoke("write_settings", { settings: updated });

      identifyOnboarding({
        role: answers.role,
        aiUsage: answers.aiUsage,
        agents: answers.agents,
      });

      // Create first project if a directory was selected.
      // This runs before the newsletter call so that a subscription error
      // (which can trigger an early return) never prevents project creation.
      if (!skipProject && projectDir) {
        try {
          const dirName =
            projectDir.split("/").filter(Boolean).pop() ?? "my-project";
          const tmpl = selectedTemplate
            ? templates.find((t) => t.name === selectedTemplate)
            : null;

          // If a template was chosen, import it (installs bundled skills).
          if (selectedTemplate) {
            await invoke("import_bundled_project_template", { name: selectedTemplate });
          }

          const mergedAgents = [
            ...new Set([...answers.agents, ...(tmpl?.agents ?? [])]),
          ];
          const project = {
            name: dirName,
            description: tmpl?.description ?? "",
            directory: projectDir,
            skills: [...(tmpl?.skills ?? [])],
            local_skills: [] as string[],
            mcp_servers: [...(tmpl?.mcp_servers ?? [])],
            providers: [...(tmpl?.providers ?? [])],
            agents: mergedAgents,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
            instruction_mode:
              tmpl?.unified_instruction?.trim() ? "unified" : "per-agent",
          };

          // save_project for new projects runs sync_project (autodetect + write agent configs).
          await invoke("save_project", {
            name: dirName,
            data: JSON.stringify(project, null, 2),
          });

          // Write unified instruction if the template provides one.
          if (tmpl?.unified_instruction?.trim()) {
            if ((tmpl.unified_rules ?? []).length > 0) {
              const latestRaw: string = await invoke("read_project", {
                name: dirName,
              });
              const latestProj = JSON.parse(latestRaw);
              const withRules = {
                ...latestProj,
                file_rules: {
                  ...(latestProj.file_rules || {}),
                  _unified: tmpl.unified_rules,
                },
              };
              await invoke("save_project", {
                name: dirName,
                data: JSON.stringify(withRules, null, 2),
              });
            }
            await invoke("save_project_file", {
              name: dirName,
              filename: "_unified",
              content: tmpl.unified_instruction,
            });
          }

          // Write any inline project files from the template.
          for (const pf of tmpl?.project_files ?? []) {
            await invoke("save_project_file", {
              name: dirName,
              filename: pf.filename,
              content: pf.content,
            });
          }

          createdProjectName = dirName;
        } catch (e) {
          console.error("[wizard] Failed to create first project:", e);
          // Non-fatal — don't block wizard completion.
        }
      }

      // Subscribe to newsletter if an email was provided.
      if (answers.email.trim()) {
        try {
          await invoke("subscribe_newsletter", { email: answers.email.trim() });
        } catch (e) {
          // Non-fatal — surface the error in the UI but don't block completion.
          console.error("[wizard] Newsletter subscription failed:", e);
          setSubscribeError(String(e));
          setSaving(false);
          return; // Stay on last step so the user sees the error.
        }
      }
    } catch (e) {
      console.error("[wizard] Failed to save wizard results:", e);
    } finally {
      setSaving(false);
      onComplete({ ...answers, createdProjectName });
    }
  };

  const handleBrowseDir = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select project directory",
    });
    if (selected) setProjectDir(selected as string);
  };

  if (loading) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg-base" />
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg-base">
      <div className="w-full max-w-xl px-6 py-10">
        {/* Logo */}
        <div className="flex items-center gap-2 mb-8">
          <img src={graphLogo} width={28} height={28} alt="Automatic" />
          <span className="text-[15px] font-semibold text-text-base">
            Automatic
          </span>
        </div>

        {/* Step indicator */}
        <StepIndicator current={step} />

        {/* Step content */}
        <div className="min-h-[360px]">
          {step === 0 && (
            <StepRole
              value={answers.role}
              onChange={(v) => setAnswers((a) => ({ ...a, role: v }))}
            />
          )}
          {step === 1 && (
            <StepAiUsage
              value={answers.aiUsage}
              onChange={(v) => setAnswers((a) => ({ ...a, aiUsage: v }))}
            />
          )}
          {step === 2 && (
            <StepAgents
              value={answers.agents}
              onChange={(v) => setAnswers((a) => ({ ...a, agents: v }))}
            />
          )}
          {step === 3 && (
            <StepEmail
              value={answers.email}
              onChange={(v) => setAnswers((a) => ({ ...a, email: v }))}
            />
          )}
          {step === 4 && (
            <StepPreferences
              analyticsEnabled={answers.analyticsEnabled}
              onToggleAnalytics={() =>
                setAnswers((a) => ({
                  ...a,
                  analyticsEnabled: !a.analyticsEnabled,
                }))
              }
            />
          )}
          {step === 5 && (
            <StepFirstProject
              templates={templates}
              selectedTemplate={selectedTemplate}
              projectDir={projectDir}
              onSelectTemplate={setSelectedTemplate}
              onBrowse={handleBrowseDir}
            />
          )}
        </div>

        {/* Subscription error */}
        {subscribeError && (
          <div className="mt-4 px-4 py-3 rounded-lg border border-danger/40 bg-danger/10 text-[12px] text-danger leading-relaxed">
            <span className="font-medium">Subscription failed:</span> {subscribeError}
            <button
              onClick={() => { setSubscribeError(null); onComplete(answers); }}
              className="ml-2 underline hover:no-underline"
            >
              Dismiss and continue
            </button>
          </div>
        )}

        {/* Navigation */}
        <div className="flex items-center justify-between mt-8 pt-6 border-t border-border-strong/40">
          {/* Back / skip */}
          <div className="flex items-center gap-4">
            {step > 0 ? (
              <button
                onClick={() => setStep((s) => s - 1)}
                className="text-[13px] text-text-muted hover:text-text-base transition-colors"
              >
                Back
              </button>
            ) : (
              <div />
            )}
            {/* Allow skipping agent selection */}
            {step === 2 && answers.agents.length === 0 && (
              <button
                onClick={() => setStep((s) => s + 1)}
                className="text-[13px] text-text-muted hover:text-text-muted transition-colors"
              >
                Skip for now
              </button>
            )}
            {/* Allow skipping first project creation */}
            {step === 5 && (
              <button
                onClick={() => finish(true)}
                disabled={saving}
                className="text-[13px] text-text-muted hover:text-text-base transition-colors"
              >
                Skip for now
              </button>
            )}
          </div>

          {/* Next / finish */}
          <button
            onClick={handleNext}
            disabled={!canAdvance() || saving}
            className="flex items-center gap-2 px-5 py-2.5 rounded-lg text-[13px] font-medium bg-brand hover:bg-brand-hover text-white shadow-sm transition-all disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {saving ? (
              "Saving..."
            ) : isLast ? (
              projectDir ? "Create & get started" : "Get started"
            ) : (
              <>
                Continue
                <ChevronRight size={14} />
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
