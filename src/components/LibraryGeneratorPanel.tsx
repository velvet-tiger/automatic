import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  Code,
  Loader2,
  MessagesSquare,
  Pencil,
  Save,
  ScrollText,
  Sparkles,
  Terminal,
  X,
} from "lucide-react";
import { MarkdownPreview } from "./MarkdownPreview";
import { useTaskLog } from "../contexts/TaskLogContext";

export type AssetKind = "skill" | "command" | "rule" | "subagent";

export interface LibraryGeneratorSavedResult {
  kind: AssetKind;
  machineName: string;
  displayName: string;
}

export interface LibraryGeneratorPanelProps {
  /** Skip the kind-selection stage and lock to this kind. */
  lockedKind?: AssetKind;
  /** Optional seed for the description textarea. */
  initialDescription?: string;
  /** Called after a successful save with the saved record's identifiers. */
  onSaved?: (result: LibraryGeneratorSavedResult) => void;
  /**
   * Called when the user cancels at the top level — from stage 1 if there is
   * no `lockedKind`, or from any stage when `lockedKind` is set. When omitted,
   * Cancel from stage 1 is a no-op (only meaningful when embedded).
   */
  onCancel?: () => void;
}

// ── Stage types ──────────────────────────────────────────────────────────────

type Stage =
  | { phase: "select" }
  | { phase: "describe"; kind: AssetKind }
  | { phase: "review"; kind: AssetKind; content: string; description: string };

// ── Kind metadata ────────────────────────────────────────────────────────────

interface KindMeta {
  kind: AssetKind;
  title: string;
  blurb: string;
  hint: string;
  icon: typeof Code;
  iconClass: string;
  iconBg: string;
  iconBorder: string;
}

const KIND_META: Record<AssetKind, KindMeta> = {
  skill: {
    kind: "skill",
    title: "Skill",
    blurb:
      "Reusable instructions loaded into agent context when activation conditions are met.",
    hint: "Describe when this skill should activate and what it should do.",
    icon: Code,
    iconClass: "text-icon-skill",
    iconBg: "bg-icon-skill/10",
    iconBorder: "border-icon-skill/20",
  },
  command: {
    kind: "command",
    title: "Command",
    blurb:
      "A slash command the agent runs as a single named workflow on demand.",
    hint: "Describe the workflow the command should perform and how it should be invoked.",
    icon: Terminal,
    iconClass: "text-icon-mcp",
    iconBg: "bg-icon-mcp/10",
    iconBorder: "border-icon-mcp/20",
  },
  rule: {
    kind: "rule",
    title: "Rule",
    blurb:
      "Short always-on guidance loaded into every agent session for this project.",
    hint: "Describe the rule's intent — what behaviour should be enforced and why.",
    icon: ScrollText,
    iconClass: "text-icon-file-template",
    iconBg: "bg-icon-file-template/10",
    iconBorder: "border-icon-file-template/20",
  },
  subagent: {
    kind: "subagent",
    title: "Sub-Agent",
    blurb:
      "A specialised agent the primary agent can invoke for focused tasks.",
    hint: "Describe the sub-agent's role, when it should be invoked, and what it should produce.",
    icon: MessagesSquare,
    iconClass: "text-icon-agent",
    iconBg: "bg-icon-agent/10",
    iconBorder: "border-icon-agent/20",
  },
};

const KIND_ORDER: AssetKind[] = ["skill", "command", "rule", "subagent"];

// ── Helpers ──────────────────────────────────────────────────────────────────

/** Slugify a free-form display name into a machine_name. */
function toMachineName(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

/** Extract the first `name:` value from a YAML frontmatter block, if present. */
function extractFrontmatterName(content: string): string | null {
  const match = content.match(/^---\s*\n([\s\S]*?)\n---/);
  if (!match) return null;
  const block = match[1] ?? "";
  const nameLine = block.split("\n").find((l) => /^\s*name\s*:/.test(l));
  if (!nameLine) return null;
  const value = nameLine.replace(/^\s*name\s*:\s*/, "").trim();
  return value.replace(/^["']|["']$/g, "");
}

/** Extract the first H1 heading from markdown, if present. */
function extractFirstH1(content: string): string | null {
  for (const line of content.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("# ")) return trimmed.slice(2).trim();
  }
  return null;
}

/** Take the first sentence (up to ~64 chars) of a description as a fallback name. */
function firstSentence(text: string): string {
  const trimmed = text.trim();
  if (!trimmed) return "";
  const stop = trimmed.search(/[.!?\n]/);
  const head = stop > 0 ? trimmed.slice(0, stop) : trimmed;
  return head.slice(0, 64).trim();
}

/**
 * Derive a sensible default display name from the generated content and the
 * user's original description, per-kind.
 */
function deriveDisplayName(
  kind: AssetKind,
  content: string,
  description: string,
): string {
  switch (kind) {
    case "skill":
    case "subagent": {
      const fm = extractFrontmatterName(content);
      if (fm) return fm;
      const h1 = extractFirstH1(content);
      if (h1) return h1;
      return firstSentence(description);
    }
    case "rule": {
      const h1 = extractFirstH1(content);
      if (h1) return h1;
      return firstSentence(description);
    }
    case "command": {
      // Commands have no `name` field — derive from description.
      return firstSentence(description);
    }
  }
}

// ── Component ────────────────────────────────────────────────────────────────

export default function LibraryGeneratorPanel({
  lockedKind,
  initialDescription = "",
  onSaved,
  onCancel,
}: LibraryGeneratorPanelProps) {
  const { log, update } = useTaskLog();

  const [stage, setStage] = useState<Stage>(
    lockedKind
      ? { phase: "describe", kind: lockedKind }
      : { phase: "select" },
  );
  const [description, setDescription] = useState(initialDescription);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Review-stage sub-state
  const [feedbackOpen, setFeedbackOpen] = useState(false);
  const [feedback, setFeedback] = useState("");
  const [saveOpen, setSaveOpen] = useState(false);
  const [saveName, setSaveName] = useState("");
  const [saving, setSaving] = useState(false);

  // Agent features gating
  const [agentEnabled, setAgentEnabled] = useState<boolean | null>(null);
  const refreshAgentEnabled = useCallback(async () => {
    try {
      const enabled = await invoke<boolean>("agent_features_enabled");
      setAgentEnabled(enabled);
    } catch {
      setAgentEnabled(false);
    }
  }, []);
  useEffect(() => {
    refreshAgentEnabled();
    const onFocus = () => refreshAgentEnabled();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refreshAgentEnabled]);

  // ── Actions ──────────────────────────────────────────────────────────────

  const handlePickKind = (kind: AssetKind) => {
    setError(null);
    setStage({ phase: "describe", kind });
  };

  const handleBackFromDescribe = () => {
    setError(null);
    if (lockedKind) {
      onCancel?.();
    } else {
      setStage({ phase: "select" });
    }
  };

  const runGeneration = async (
    kind: AssetKind,
    prevAttempt?: string,
    fb?: string,
  ): Promise<string | null> => {
    const logId = log(
      prevAttempt
        ? `Revising ${KIND_META[kind].title.toLowerCase()}…`
        : `Generating ${KIND_META[kind].title.toLowerCase()}…`,
      "running",
    );
    setGenerating(true);
    setError(null);
    try {
      const content = await invoke<string>("ai_generate_library_asset", {
        kind,
        description,
        previousAttempt: prevAttempt ?? null,
        feedback: fb ?? null,
      });
      update(logId, `${KIND_META[kind].title} draft ready`, "success");
      return content;
    } catch (e) {
      const msg = String(e);
      update(logId, msg, "error");
      setError(msg);
      return null;
    } finally {
      setGenerating(false);
    }
  };

  const handleGenerate = async () => {
    if (stage.phase !== "describe") return;
    const content = await runGeneration(stage.kind);
    if (content !== null) {
      setStage({
        phase: "review",
        kind: stage.kind,
        content,
        description,
      });
      setFeedbackOpen(false);
      setFeedback("");
    }
  };

  const handleApplyFeedback = async () => {
    if (stage.phase !== "review" || !feedback.trim()) return;
    const content = await runGeneration(stage.kind, stage.content, feedback);
    if (content !== null) {
      setStage({ ...stage, content });
      setFeedbackOpen(false);
      setFeedback("");
    }
  };

  const handleCancelReview = () => {
    setSaveOpen(false);
    setFeedbackOpen(false);
    setFeedback("");
    setError(null);
    if (lockedKind) {
      onCancel?.();
    } else {
      setStage({ phase: "select" });
      setDescription("");
    }
  };

  const handleOpenSave = () => {
    if (stage.phase !== "review") return;
    setSaveName(deriveDisplayName(stage.kind, stage.content, stage.description));
    setSaveOpen(true);
  };

  const handleConfirmSave = async () => {
    if (stage.phase !== "review") return;
    const trimmed = saveName.trim();
    if (!trimmed) {
      setError("Provide a name before saving.");
      return;
    }
    const machineName = toMachineName(trimmed);
    if (!machineName) {
      setError("Name must contain at least one letter or digit.");
      return;
    }
    const logId = log(
      `Saving ${KIND_META[stage.kind].title.toLowerCase()} '${trimmed}'…`,
      "running",
    );
    setSaving(true);
    setError(null);
    try {
      switch (stage.kind) {
        case "skill":
          await invoke("save_skill", { name: machineName, content: stage.content });
          break;
        case "command":
          await invoke("save_user_command", { machineName, content: stage.content });
          break;
        case "rule":
          await invoke("save_rule", {
            machineName,
            name: trimmed,
            content: stripFirstH1ForRule(stage.content),
          });
          break;
        case "subagent":
          await invoke("save_subagent", {
            machineName,
            name: trimmed,
            content: stage.content,
          });
          break;
      }
      update(logId, `${KIND_META[stage.kind].title} '${trimmed}' saved`, "success");
      setSaveOpen(false);
      onSaved?.({ kind: stage.kind, machineName, displayName: trimmed });
      if (!lockedKind) {
        setStage({ phase: "select" });
        setDescription("");
      }
    } catch (e) {
      const msg = String(e);
      update(logId, msg, "error");
      setError(msg);
    } finally {
      setSaving(false);
    }
  };

  // ── Render helpers ───────────────────────────────────────────────────────

  const disabledBanner = agentEnabled === false && (
    <div className="rounded-lg border border-warning/40 bg-warning/10 px-4 py-2.5 text-[12px] text-text-base">
      Agent features are disabled. Enable them in Settings → Agents to use this tool.
    </div>
  );

  const meta = useMemo(
    () =>
      stage.phase === "select" ? null : KIND_META[stage.kind],
    [stage],
  );

  return (
    <div className="flex flex-col gap-6">
      {disabledBanner}

      {error && (
        <div className="rounded-lg border border-danger/40 bg-danger/10 px-4 py-2.5 text-[12px] text-text-base">
          {error}
        </div>
      )}

      {/* ── Stage 1: kind selection ─────────────────────────────────── */}
      {stage.phase === "select" && (
        <div className="grid grid-cols-2 gap-4">
          {KIND_ORDER.map((kind) => {
            const m = KIND_META[kind];
            const Icon = m.icon;
            return (
              <button
                key={kind}
                onClick={() => handlePickKind(kind)}
                className="bg-bg-input border border-border-strong/40 rounded-xl p-6 text-left hover:border-brand/50 hover:bg-surface-hover transition-all group flex flex-col"
              >
                <div className="flex items-start gap-3 mb-3">
                  <div
                    className={`p-2.5 ${m.iconBg} rounded-lg border ${m.iconBorder} flex-shrink-0 transition-colors`}
                  >
                    <Icon size={20} className={m.iconClass} />
                  </div>
                  <div className="flex-1 min-w-0">
                    <h3 className="text-[15px] font-semibold text-text-base leading-snug pt-0.5">
                      {m.title}
                    </h3>
                  </div>
                </div>
                <p className="text-[12px] text-text-muted leading-relaxed flex-1">
                  {m.blurb}
                </p>
              </button>
            );
          })}
        </div>
      )}

      {/* ── Stage 2: describe ───────────────────────────────────────── */}
      {stage.phase === "describe" && meta && (
        <div className="flex flex-col gap-3">
          <KindHeader meta={meta} />
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={meta.hint}
            rows={8}
            className="w-full text-[13px] text-text-base bg-bg-input border border-border-strong/40 rounded-md px-3 py-2 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 resize-y"
          />
          <div className="flex items-center justify-between">
            <button
              onClick={handleBackFromDescribe}
              className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
            >
              <ArrowLeft size={12} />
              {lockedKind ? "Cancel" : "Back"}
            </button>
            <button
              onClick={handleGenerate}
              disabled={
                generating ||
                !description.trim() ||
                agentEnabled === false
              }
              title={
                agentEnabled === false
                  ? "Enable Agent features in Settings → Agents"
                  : undefined
              }
              className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md text-[12px] font-medium bg-brand hover:bg-brand-hover text-white disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {generating ? <Loader2 size={12} className="animate-spin" /> : <Sparkles size={12} />}
              Generate
            </button>
          </div>
        </div>
      )}

      {/* ── Stage 3: review ─────────────────────────────────────────── */}
      {stage.phase === "review" && meta && (
        <div className="flex flex-col gap-4">
          <KindHeader meta={meta} />

          <div className="rounded-lg border border-border-strong/40 bg-bg-input p-4 max-h-[480px] overflow-y-auto custom-scrollbar">
            <MarkdownPreview content={stage.content} />
          </div>

          {/* Make-changes inline editor */}
          {feedbackOpen && (
            <div className="flex flex-col gap-2 rounded-lg border border-border-strong/40 bg-bg-input p-3">
              <label className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
                What should change?
              </label>
              <textarea
                value={feedback}
                onChange={(e) => setFeedback(e.target.value)}
                placeholder="Describe the changes you want…"
                rows={4}
                className="w-full text-[13px] text-text-base bg-bg-base border border-border-strong/40 rounded-md px-3 py-2 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 resize-y"
              />
              <div className="flex items-center justify-end gap-2">
                <button
                  onClick={() => { setFeedbackOpen(false); setFeedback(""); }}
                  className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
                >
                  Discard
                </button>
                <button
                  onClick={handleApplyFeedback}
                  disabled={generating || !feedback.trim()}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-brand hover:bg-brand-hover text-white disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {generating ? <Loader2 size={12} className="animate-spin" /> : <Sparkles size={12} />}
                  Apply changes
                </button>
              </div>
            </div>
          )}

          {/* Save-as dialog */}
          {saveOpen && (
            <div className="flex flex-col gap-2 rounded-lg border border-border-strong/40 bg-bg-input p-3">
              <label className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
                Save as
              </label>
              <input
                type="text"
                value={saveName}
                onChange={(e) => setSaveName(e.target.value)}
                placeholder="Display name"
                autoFocus
                className="w-full text-[13px] text-text-base bg-bg-base border border-border-strong/40 rounded-md px-3 py-2 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60"
              />
              <p className="text-[11px] text-text-muted">
                Saved as <span className="font-mono text-text-base">{toMachineName(saveName) || "—"}</span>
              </p>
              <div className="flex items-center justify-end gap-2 pt-1">
                <button
                  onClick={() => setSaveOpen(false)}
                  className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={handleConfirmSave}
                  disabled={saving || !saveName.trim()}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-brand hover:bg-brand-hover text-white disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {saving ? <Loader2 size={12} className="animate-spin" /> : <Save size={12} />}
                  Save to Library
                </button>
              </div>
            </div>
          )}

          {/* Primary action row */}
          {!saveOpen && (
            <div className="flex items-center justify-between">
              <button
                onClick={handleCancelReview}
                className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
              >
                <X size={12} />
                Cancel
              </button>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setFeedbackOpen((open) => !open)}
                  disabled={generating}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium border border-border-strong/40 hover:border-border-strong/80 text-text-base transition-colors disabled:opacity-50"
                >
                  <Pencil size={12} />
                  {feedbackOpen ? "Hide changes" : "Make changes"}
                </button>
                <button
                  onClick={handleOpenSave}
                  disabled={generating}
                  className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md text-[12px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors disabled:opacity-50"
                >
                  <Save size={12} />
                  Save to Library
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** A rule's first H1 becomes its display name and is stripped from the saved body. */
function stripFirstH1ForRule(content: string): string {
  const lines = content.split("\n");
  const idx = lines.findIndex((l) => l.trim().startsWith("# "));
  if (idx === -1) return content.trim() + "\n";
  // Drop the H1 line plus any immediately-following blank line.
  const without = [...lines.slice(0, idx), ...lines.slice(idx + 1)];
  while (without.length > 0 && without[0]!.trim() === "") without.shift();
  return without.join("\n").trim() + "\n";
}

function KindHeader({ meta }: { meta: KindMeta }) {
  const Icon = meta.icon;
  return (
    <div className="flex items-center gap-2.5">
      <div className={`p-1.5 ${meta.iconBg} rounded-md border ${meta.iconBorder}`}>
        <Icon size={14} className={meta.iconClass} />
      </div>
      <span className="text-[13px] font-medium text-text-base">{meta.title}</span>
    </div>
  );
}
