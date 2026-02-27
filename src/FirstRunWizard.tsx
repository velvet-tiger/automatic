import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { identifyOnboarding } from "./analytics";
import { ChevronRight, Check } from "lucide-react";
import graphLogo from "../logos/graph_5.svg";

// ── Types ─────────────────────────────────────────────────────────────────────

interface WizardAnswers {
  role: string;
  aiUsage: string;
  agents: string[];
  email: string;
  analyticsEnabled: boolean;
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
  { id: "claude", label: "Claude Code" },
  { id: "cursor", label: "Cursor" },
  { id: "github_copilot", label: "GitHub Copilot" },
  { id: "cline", label: "Cline" },
  { id: "kilo_code", label: "Kilo Code" },
  { id: "junie", label: "Junie" },
  { id: "kiro", label: "Kiro" },
  { id: "gemini_cli", label: "Gemini CLI" },
  { id: "codex_cli", label: "Codex CLI" },
  { id: "goose", label: "Goose" },
  { id: "opencode", label: "OpenCode" },
  { id: "warp", label: "Warp" },
  { id: "antigravity", label: "Antigravity" },
  { id: "droid", label: "Droid" },
  { id: "other", label: "Other" },
];

// ── Step indicator ────────────────────────────────────────────────────────────

const STEPS = ["Your role", "AI workflow", "Your agents", "Stay in touch", "Preferences"];

function StepIndicator({ current }: { current: number }) {
  return (
    <div className="flex items-center gap-2 mb-10">
      {STEPS.map((label, i) => {
        const done = i < current;
        const active = i === current;
        return (
          <div key={i} className="flex items-center gap-2">
            <div className="flex items-center gap-1.5">
              <div
                className={`w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-semibold transition-colors ${
                  done
                    ? "bg-[#5E6AD2] text-white"
                    : active
                      ? "border-2 border-[#5E6AD2] text-[#5E6AD2]"
                      : "border border-[#3E4048] text-[#555760]"
                }`}
              >
                {done ? <Check size={10} strokeWidth={3} /> : i + 1}
              </div>
              <span
                className={`text-[12px] hidden sm:inline ${
                  active
                    ? "text-[#F8F8FA] font-medium"
                    : done
                      ? "text-[#5E6AD2]"
                      : "text-[#555760]"
                }`}
              >
                {label}
              </span>
            </div>
            {i < STEPS.length - 1 && (
              <div
                className={`w-6 h-px ${done ? "bg-[#5E6AD2]" : "bg-[#3E4048]"}`}
              />
            )}
          </div>
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
          ? "border-[#5E6AD2] bg-[#5E6AD2]/10"
          : "border-[#3E4048] bg-[#18191C] hover:border-[#5E5E6A] hover:bg-[#1E1F24]"
      }`}
    >
      <div
        className={`mt-0.5 w-4 h-4 rounded-full border-2 flex-shrink-0 flex items-center justify-center transition-colors ${
          selected ? "border-[#5E6AD2] bg-[#5E6AD2]" : "border-[#555760]"
        }`}
      >
        {selected && <div className="w-1.5 h-1.5 rounded-full bg-white" />}
      </div>
      <div className="min-w-0">
        <div className="text-[13px] font-medium text-[#F8F8FA]">{label}</div>
        {description && (
          <div className="text-[12px] text-[#C8CAD0] mt-0.5 leading-relaxed">
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
          ? "border-[#5E6AD2] bg-[#5E6AD2]/15 text-[#8A94F5]"
          : "border-[#3E4048] bg-[#18191C] text-[#C8CAD0] hover:border-[#5E5E6A] hover:text-[#F8F8FA]"
      }`}
    >
      {selected && <Check size={12} className="text-[#5E6AD2]" strokeWidth={3} />}
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
        enabled ? "bg-[#5E6AD2]" : "bg-[#3E4048]"
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
      <h2 className="text-[22px] font-semibold text-[#F8F8FA] mb-1">
        What's your primary role?
      </h2>
      <p className="text-[14px] text-[#C8CAD0] mb-6 leading-relaxed">
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
      <h2 className="text-[22px] font-semibold text-[#F8F8FA] mb-1">
        How do you use AI in your development process?
      </h2>
      <p className="text-[14px] text-[#C8CAD0] mb-6 leading-relaxed">
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
      <h2 className="text-[22px] font-semibold text-[#F8F8FA] mb-1">
        Which AI agents do you work with?
      </h2>
      <p className="text-[14px] text-[#C8CAD0] mb-6 leading-relaxed">
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
        <p className="mt-4 text-[12px] text-[#555760] italic">
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
      <h2 className="text-[22px] font-semibold text-[#F8F8FA] mb-1">
        Stay in the loop
      </h2>
      <p className="text-[14px] text-[#C8CAD0] mb-6 leading-relaxed">
        Get occasional updates on new features and improvements to Automatic.
        We won't spam you — unsubscribe any time.
      </p>
      <div className="space-y-2">
        <label className="block text-[12px] font-medium text-[#C8CAD0]">
          Email address <span className="text-[#555760]">(optional)</span>
        </label>
        <input
          type="email"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder="you@example.com"
          className="w-full px-3 py-2.5 rounded-lg border border-[#3E4048] bg-[#18191C] text-[13px] text-[#F8F8FA] placeholder-[#555760] focus:outline-none focus:border-[#5E6AD2] focus:ring-1 focus:ring-[#5E6AD2]/40 transition-colors"
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
      <h2 className="text-[22px] font-semibold text-[#F8F8FA] mb-1">
        A couple of quick preferences
      </h2>
      <p className="text-[14px] text-[#C8CAD0] mb-6 leading-relaxed">
        You can change these at any time in Settings.
      </p>

      <div className="space-y-3">
        <button
          onClick={onToggleAnalytics}
          className={`flex items-center justify-between w-full p-4 rounded-lg border text-left transition-all ${
            analyticsEnabled
              ? "border-[#5E6AD2] bg-[#5E6AD2]/10"
              : "border-[#3E4048] bg-[#18191C] hover:border-[#5E5E6A] hover:bg-[#1E1F24]"
          }`}
        >
          <div>
            <div className="text-[13px] font-medium text-[#F8F8FA]">
              Share anonymous usage data
            </div>
            <div className="text-[12px] text-[#C8CAD0] mt-0.5 leading-relaxed">
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

  const totalSteps = STEPS.length;
  const isLast = step === totalSteps - 1;

  const canAdvance = () => {
    if (step === 0) return answers.role !== "";
    if (step === 1) return answers.aiUsage !== "";
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

  const finish = async () => {
    setSaving(true);
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
      onComplete(answers);
    }
  };

  if (loading) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-[#111215]" />
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-[#111215]">
      <div className="w-full max-w-xl px-6 py-10">
        {/* Logo */}
        <div className="flex items-center gap-2 mb-8">
          <img src={graphLogo} width={28} height={28} alt="Automatic" />
          <span className="text-[15px] font-semibold text-[#F8F8FA]">
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
        </div>

        {/* Subscription error */}
        {subscribeError && (
          <div className="mt-4 px-4 py-3 rounded-lg border border-[#E05252]/40 bg-[#E05252]/10 text-[12px] text-[#E05252] leading-relaxed">
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
        <div className="flex items-center justify-between mt-8 pt-6 border-t border-[#2A2B30]">
          {/* Back / skip */}
          <div className="flex items-center gap-4">
            {step > 0 ? (
              <button
                onClick={() => setStep((s) => s - 1)}
                className="text-[13px] text-[#C8CAD0] hover:text-[#F8F8FA] transition-colors"
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
                className="text-[13px] text-[#555760] hover:text-[#C8CAD0] transition-colors"
              >
                Skip for now
              </button>
            )}
          </div>

          {/* Next / finish */}
          <button
            onClick={handleNext}
            disabled={!canAdvance() || saving}
            className="flex items-center gap-2 px-5 py-2.5 rounded-lg text-[13px] font-medium bg-[#5E6AD2] hover:bg-[#6B78E3] text-white shadow-sm transition-all disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {saving ? (
              "Saving..."
            ) : isLast ? (
              "Get started"
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
