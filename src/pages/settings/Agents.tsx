import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, ChevronDown, ChevronRight, Eye, EyeOff, Key, Trash2 } from "lucide-react";
import { AgentIcon } from "../../components/AgentIcon";

/**
 * An LLM agent that Automatic can use for in-app features
 * (file generation, insight generation, AI Playground, etc.).
 *
 * Each agent maps to a credential provider whose API key is stored
 * in the OS keychain via the existing `save_api_key` / `has_api_key` /
 * `delete_api_key` Tauri commands.
 */
interface AgentDefinition {
  id: string;
  label: string;
  description: string;
  provider: string;
  providerLabel: string;
  placeholder: string;
  /** When present, the credential card renders a second input for account ID. */
  accountIdPlaceholder?: string;
  /** When true, the card shows a Cloudflare AI Gateway configuration section. */
  supportsGateway?: boolean;
}

interface GatewayConfig {
  account_id: string;
  gateway_id: string;
  cf_token?: string;
}

interface GatewayFormState {
  expanded: boolean;
  accountId: string;
  gatewayId: string;
  cfToken: string;
  saveStatus: "idle" | "saving" | "saved" | "error";
}

const AGENTS: AgentDefinition[] = [
  {
    id: "claude",
    label: "Claude",
    description: "Anthropic's Claude models. Used for file generation, insight generation, recommendations, and the AI Playground.",
    provider: "anthropic",
    providerLabel: "Anthropic",
    placeholder: "sk-ant-...",
    supportsGateway: true,
  },
  {
    id: "openai",
    label: "OpenAI",
    description: "OpenAI's GPT models. Powers file generation, insight generation, recommendations, and the AI Playground.",
    provider: "openai",
    providerLabel: "OpenAI",
    placeholder: "sk-...",
  },
  {
    id: "github-models",
    label: "GitHub Models",
    description: "OpenAI-compatible models hosted by GitHub via GitHub Copilot. Supports GPT-4.1 and other models for file generation, insight generation, recommendations, and the AI Playground.",
    provider: "github-models",
    providerLabel: "GitHub / Copilot",
    placeholder: "ghp_...",
  },
  {
    id: "zai",
    label: "Z.ai",
    description: "Z.ai's GLM models via the OpenAI-compatible API. Powers file generation, insight generation, recommendations, and the AI Playground.",
    provider: "zai",
    providerLabel: "Z.ai",
    placeholder: "API key",
  },
  {
    id: "opencode-zen",
    label: "OpenCode Zen",
    description: "OpenCode Zen aggregates models from Anthropic, OpenAI, Google, and others under a single API key. Powers file generation, insight generation, recommendations, and the AI Playground.",
    provider: "opencode-zen",
    providerLabel: "OpenCode",
    placeholder: "API key",
  },
  {
    id: "workers-ai",
    label: "Workers AI",
    description: "Cloudflare Workers AI models via the OpenAI-compatible API. Powers file generation, insight generation, recommendations, and the AI Playground.",
    provider: "workers-ai",
    providerLabel: "Cloudflare",
    placeholder: "Bearer token",
    accountIdPlaceholder: "Account ID",
    supportsGateway: true,
  },
];

type SaveStatus = "idle" | "saving" | "saved" | "error";

interface KeyState {
  stored: boolean;
  inputValue: string;
  /** Second input value — used for account ID on TokenAndAccount providers (e.g. Workers AI). */
  inputValue2: string;
  editing: boolean;
  revealed: boolean;
  saveStatus: SaveStatus;
}

function defaultKeyState(): KeyState {
  return { stored: false, inputValue: "", inputValue2: "", editing: false, revealed: false, saveStatus: "idle" };
}

export default function SettingsAgents() {
  const [keyStates, setKeyStates] = useState<Record<string, KeyState>>({});
  /** Cached model lists keyed by provider, loaded on demand when a key is stored. */
  const [agentModels, setAgentModels] = useState<Record<string, string[]>>({});
  /** User's selected model per provider, mirrors settings.agent_models. */
  const [selectedModels, setSelectedModels] = useState<Record<string, string>>({});
  /**
   * The master toggle's stored value. `null` means it has never been set
   * explicitly, in which case the effective state follows whether any key is
   * stored (preserves the pre-toggle behaviour for upgrading users).
   */
  const [enabledOverride, setEnabledOverride] = useState<boolean | null>(null);
  /**
   * The stored active agent ID. `null` means it has never been set; the
   * effective active agent defaults to `"anthropic"` in that case.
   */
  const [activeAgent, setActiveAgent] = useState<string | null>(null);
  /** Gateway configs loaded from settings, keyed by provider. */
  const [savedGateways, setSavedGateways] = useState<Record<string, GatewayConfig>>({});
  /** Per-provider gateway form state (only relevant for supportsGateway agents). */
  const [gatewayStates, setGatewayStates] = useState<Record<string, GatewayFormState>>({});

  useEffect(() => {
    loadKeyStatus();
    loadSettings();
  }, []);

  const loadKeyStatus = async () => {
    const results = await Promise.all(
      AGENTS.map(async (agent) => {
        try {
          const stored = await invoke<boolean>("has_api_key", { provider: agent.provider });
          return { provider: agent.provider, stored };
        } catch {
          return { provider: agent.provider, stored: false };
        }
      })
    );
    const states: Record<string, KeyState> = {};
    for (const { provider, stored } of results) {
      states[provider] = { ...defaultKeyState(), stored };
    }
    setKeyStates(states);
    // Pre-load model lists for all configured providers.
    for (const { provider, stored } of results) {
      if (stored) loadModelsForProvider(provider);
    }
  };

  const loadSettings = async () => {
    try {
      const raw = await invoke<{
        agent_features_enabled?: boolean | null;
        active_agent?: string | null;
        agent_models?: Record<string, string> | null;
        agent_gateways?: Record<string, GatewayConfig> | null;
      }>("read_settings");
      setEnabledOverride(raw.agent_features_enabled ?? null);
      setActiveAgent(raw.active_agent ?? null);
      setSelectedModels(raw.agent_models ?? {});
      const gateways = raw.agent_gateways ?? {};
      setSavedGateways(gateways);
      // Pre-fill gateway form state from saved configs.
      setGatewayStates((prev) => {
        const next = { ...prev };
        for (const provider of Object.keys(gateways)) {
          const saved = gateways[provider];
          next[provider] = {
            expanded: false,
            accountId: saved.account_id,
            gatewayId: saved.gateway_id,
            cfToken: saved.cf_token ?? "",
            saveStatus: "idle",
          };
        }
        return next;
      });
    } catch (e) {
      console.error("Failed to read settings", e);
    }
  };

  const loadModelsForProvider = async (provider: string) => {
    if (agentModels[provider]) return;
    try {
      const models = await invoke<string[]>("list_agent_models", { agentId: provider });
      setAgentModels((prev) => ({ ...prev, [provider]: models }));
    } catch {
      // non-fatal — model selector stays hidden
    }
  };

  const persistSelectedModel = async (provider: string, model: string) => {
    try {
      const raw = await invoke<Record<string, unknown>>("read_settings");
      const existing = (raw.agent_models as Record<string, string> | undefined) ?? {};
      raw.agent_models = { ...existing, [provider]: model };
      await invoke("write_settings", { settings: raw });
      setSelectedModels((prev) => ({ ...prev, [provider]: model }));
    } catch (e) {
      console.error("Failed to save model preference", e);
    }
  };

  const persistEnabledOverride = async (value: boolean) => {
    try {
      const raw = await invoke<Record<string, unknown>>("read_settings");
      raw.agent_features_enabled = value;
      await invoke("write_settings", { settings: raw });
      setEnabledOverride(value);
    } catch (e) {
      console.error("Failed to write agent_features_enabled", e);
    }
  };

  const persistActiveAgent = async (agentId: string) => {
    try {
      const raw = await invoke<Record<string, unknown>>("read_settings");
      raw.active_agent = agentId;
      await invoke("write_settings", { settings: raw });
      setActiveAgent(agentId);
    } catch (e) {
      console.error("Failed to write active_agent", e);
    }
  };

  const defaultGatewayState = (): GatewayFormState => ({
    expanded: false,
    accountId: "",
    gatewayId: "",
    cfToken: "",
    saveStatus: "idle",
  });

  const updateGatewayState = (provider: string, patch: Partial<GatewayFormState>) => {
    setGatewayStates((prev) => ({
      ...prev,
      [provider]: { ...(prev[provider] ?? defaultGatewayState()), ...patch },
    }));
  };

  const saveGateway = async (provider: string) => {
    const gs = gatewayStates[provider];
    if (!gs || !gs.accountId.trim() || !gs.gatewayId.trim()) return;
    updateGatewayState(provider, { saveStatus: "saving" });
    try {
      const raw = await invoke<Record<string, unknown>>("read_settings");
      const existing = (raw.agent_gateways as Record<string, GatewayConfig> | undefined) ?? {};
      const config: GatewayConfig = {
        account_id: gs.accountId.trim(),
        gateway_id: gs.gatewayId.trim(),
        ...(gs.cfToken.trim() ? { cf_token: gs.cfToken.trim() } : {}),
      };
      raw.agent_gateways = { ...existing, [provider]: config };
      await invoke("write_settings", { settings: raw });
      setSavedGateways((prev) => ({ ...prev, [provider]: config }));
      updateGatewayState(provider, { saveStatus: "saved", expanded: false });
      setTimeout(() => updateGatewayState(provider, { saveStatus: "idle" }), 2000);
    } catch (e) {
      console.error("Failed to save gateway config", e);
      updateGatewayState(provider, { saveStatus: "error" });
      setTimeout(() => updateGatewayState(provider, { saveStatus: "idle" }), 3000);
    }
  };

  const clearGateway = async (provider: string) => {
    try {
      const raw = await invoke<Record<string, unknown>>("read_settings");
      const existing = { ...((raw.agent_gateways as Record<string, GatewayConfig> | undefined) ?? {}) };
      delete existing[provider];
      raw.agent_gateways = existing;
      await invoke("write_settings", { settings: raw });
      setSavedGateways((prev) => {
        const next = { ...prev };
        delete next[provider];
        return next;
      });
      setGatewayStates((prev) => ({
        ...prev,
        [provider]: defaultGatewayState(),
      }));
    } catch (e) {
      console.error("Failed to clear gateway config", e);
    }
  };

  const anyKeyStored = AGENTS.some((a) => keyStates[a.provider]?.stored);
  const featuresEnabled = enabledOverride ?? anyKeyStored;

  const effectiveActiveAgent = activeAgent ?? "anthropic";

  const updateState = (provider: string, patch: Partial<KeyState>) => {
    setKeyStates((prev) => ({
      ...prev,
      [provider]: { ...(prev[provider] ?? defaultKeyState()), ...patch },
    }));
  };

  const saveKey = async (provider: string) => {
    const state = keyStates[provider];
    const agent = AGENTS.find((a) => a.provider === provider);
    const isTokenAndAccount = !!agent?.accountIdPlaceholder;
    if (!state || !state.inputValue.trim()) return;
    if (isTokenAndAccount && !state.inputValue2.trim()) return;
    updateState(provider, { saveStatus: "saving" });
    const key = isTokenAndAccount
      ? JSON.stringify({ token: state.inputValue.trim(), account_id: state.inputValue2.trim() })
      : state.inputValue.trim();
    try {
      await invoke("save_api_key", { provider, key });
      updateState(provider, {
        stored: true,
        inputValue: "",
        inputValue2: "",
        editing: false,
        revealed: false,
        saveStatus: "saved",
      });
      // The backend auto-enables features the first time a key is added.
      await loadSettings();
      loadModelsForProvider(provider);
      setTimeout(() => updateState(provider, { saveStatus: "idle" }), 2000);
    } catch (e) {
      console.error(`Failed to save API key for ${provider}`, e);
      updateState(provider, { saveStatus: "error" });
      setTimeout(() => updateState(provider, { saveStatus: "idle" }), 3000);
    }
  };

  const deleteKey = async (provider: string) => {
    try {
      await invoke("delete_api_key", { provider });
      updateState(provider, {
        stored: false,
        inputValue: "",
        inputValue2: "",
        editing: false,
        revealed: false,
        saveStatus: "idle",
      });
      // The backend auto-disables features when the last key is removed.
      await loadSettings();
    } catch (e) {
      console.error(`Failed to delete API key for ${provider}`, e);
    }
  };

  return (
    <div>
      <h2 className="text-lg font-medium mb-1 text-text-base">Agents</h2>
      <p className="text-[13px] text-text-muted mb-6 leading-relaxed">
        Agents power Automatic's in-app AI features &mdash; file generation, insight generation,
        recommendations, and the AI Playground. These are independent of the Providers used to
        configure your projects.
      </p>

      <button
        onClick={() => persistEnabledOverride(!featuresEnabled)}
        className={`flex items-center justify-between w-full p-4 rounded-lg border text-left transition-all mb-6 ${
          featuresEnabled
            ? "border-brand bg-brand/10"
            : "border-border-strong/40 bg-bg-input-dark hover:border-border-strong hover:bg-surface-hover"
        }`}
      >
        <div>
          <div className="text-[13px] font-medium text-text-base">Agent features</div>
          <div className="text-[12px] text-text-muted">
            {featuresEnabled
              ? "Enabled — in-app AI features are active"
              : "Disabled — in-app AI features will not run"}
          </div>
        </div>
        <div
          className={`relative flex-shrink-0 w-10 h-5 rounded-full transition-colors ${
            featuresEnabled ? "bg-brand" : "bg-surface-active"
          }`}
        >
          <div
            className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-all ${
              featuresEnabled ? "left-5" : "left-0.5"
            }`}
          />
        </div>
      </button>

      {anyKeyStored && (
        <section className="mb-6">
          <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
            Active agent
          </label>
          <div className="rounded-lg border border-border-strong/40 bg-bg-input overflow-hidden divide-y divide-border-strong/20">
            {AGENTS.map((agent) => {
              const isActive = effectiveActiveAgent === agent.provider;
              const hasKey = keyStates[agent.provider]?.stored ?? false;
              return (
                <button
                  key={agent.id}
                  onClick={() => hasKey ? persistActiveAgent(agent.provider) : undefined}
                  disabled={!hasKey}
                  className={`flex items-center gap-3 w-full px-3 py-2.5 text-left transition-colors ${
                    isActive ? "bg-brand/10" : hasKey ? "hover:bg-surface-hover" : "opacity-40 cursor-default"
                  }`}
                >
                  <AgentIcon agentId={agent.id} size={18} />
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] font-medium text-text-base">{agent.label}</div>
                    <div className="text-[11px] text-text-muted">
                      {hasKey ? agent.providerLabel : "No key configured"}
                    </div>
                  </div>
                  <div
                    className={`w-3.5 h-3.5 rounded-full border-2 flex-shrink-0 transition-colors ${
                      isActive ? "border-brand bg-brand" : "border-border-strong/60 bg-transparent"
                    }`}
                  />
                </button>
              );
            })}
          </div>
          <p className="text-[11px] text-text-muted mt-2 leading-relaxed">
            The selected agent powers all in-app AI features.
          </p>
        </section>
      )}

      <div className="space-y-4">
        {AGENTS.map((agent) => {
          const state = keyStates[agent.provider] ?? defaultKeyState();
          return (
            <section key={agent.id}>
              <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3 flex items-center gap-1.5">
                <Key size={12} className="text-text-muted" /> {agent.label}
              </label>

              <div className="rounded-lg border border-border-strong/40 bg-bg-input overflow-hidden">
                <div className="flex items-center gap-3 px-3 py-3">
                  <AgentIcon agentId={agent.id} size={20} />
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] font-medium text-text-base">
                      {agent.providerLabel}
                    </div>
                    <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
                      {agent.description}
                    </div>
                  </div>

                  {state.stored && !state.editing && (
                    <span className="flex items-center gap-1 text-[11px] text-success font-medium">
                      <Check size={11} /> Stored
                    </span>
                  )}
                  {!state.stored && !state.editing && (
                    <span className="text-[11px] text-text-muted">Not configured</span>
                  )}

                  {!state.editing && (
                    <div className="flex items-center gap-1.5">
                      <button
                        onClick={() => updateState(agent.provider, { editing: true, inputValue: "", inputValue2: "", revealed: false })}
                        className="px-2.5 py-1 text-[11px] font-medium rounded border border-brand/50 text-brand hover:border-brand hover:bg-brand/15 transition-all"
                      >
                        {state.stored ? "Update" : "Add Key"}
                      </button>
                      {state.stored && (
                        <button
                          onClick={() => deleteKey(agent.provider)}
                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-all"
                          title="Remove stored key"
                        >
                          <Trash2 size={12} />
                        </button>
                      )}
                    </div>
                  )}
                </div>

                {state.stored && !state.editing && agentModels[agent.provider]?.length > 0 && (
                  <div className="border-t border-border-strong/20 px-3 py-2.5 flex items-center gap-2">
                    <span className="text-[11px] text-text-muted flex-shrink-0">Model</span>
                    <div className="relative flex-1">
                      <select
                        value={selectedModels[agent.provider] ?? ""}
                        onChange={(e) => persistSelectedModel(agent.provider, e.target.value)}
                        className="w-full appearance-none text-[12px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 pr-7 py-1 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
                      >
                        <option value="">Default</option>
                        {agentModels[agent.provider]!.map((m) => (
                          <option key={m} value={m}>{m}</option>
                        ))}
                      </select>
                      <ChevronDown size={12} className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted" />
                    </div>
                  </div>
                )}

                {agent.supportsGateway && state.stored && !state.editing && (() => {
                  const gs = gatewayStates[agent.provider] ?? defaultGatewayState();
                  const saved = savedGateways[agent.provider];
                  return (
                    <div className="border-t border-border-strong/20">
                      <button
                        onClick={() => updateGatewayState(agent.provider, { expanded: !gs.expanded })}
                        className="flex items-center gap-2 w-full px-3 py-2.5 text-left hover:bg-surface-hover transition-colors"
                      >
                        {gs.expanded
                          ? <ChevronDown size={11} className="text-text-muted flex-shrink-0" />
                          : <ChevronRight size={11} className="text-text-muted flex-shrink-0" />
                        }
                        <span className="text-[11px] text-text-muted flex-1">Cloudflare AI Gateway</span>
                        {saved ? (
                          <span className="text-[11px] text-success font-medium">Active</span>
                        ) : (
                          <span className="text-[11px] text-text-muted">Off</span>
                        )}
                      </button>
                      {gs.expanded && (
                        <div className="px-3 pb-3 space-y-2">
                          <input
                            type="text"
                            value={gs.accountId}
                            onChange={(e) => updateGatewayState(agent.provider, { accountId: e.target.value })}
                            placeholder="Account ID"
                            className="w-full text-[12px] text-text-base bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand transition-colors"
                          />
                          <input
                            type="text"
                            value={gs.gatewayId}
                            onChange={(e) => updateGatewayState(agent.provider, { gatewayId: e.target.value })}
                            placeholder="Gateway ID"
                            className="w-full text-[12px] text-text-base bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand transition-colors"
                          />
                          <input
                            type="text"
                            value={gs.cfToken}
                            onChange={(e) => updateGatewayState(agent.provider, { cfToken: e.target.value })}
                            placeholder="CF AIG Token (optional)"
                            className="w-full text-[12px] text-text-base bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand transition-colors"
                          />
                          <div className="flex items-center gap-2 pt-1">
                            <button
                              onClick={() => saveGateway(agent.provider)}
                              disabled={!gs.accountId.trim() || !gs.gatewayId.trim() || gs.saveStatus === "saving"}
                              className="px-3 py-1.5 text-[11px] font-medium rounded bg-brand text-white hover:bg-brand-active disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                            >
                              {gs.saveStatus === "saving" ? "Saving..." : "Save"}
                            </button>
                            {saved && (
                              <button
                                onClick={() => clearGateway(agent.provider)}
                                className="px-3 py-1.5 text-[11px] font-medium rounded border border-danger/50 text-danger hover:border-danger hover:bg-danger/10 transition-all"
                              >
                                Clear
                              </button>
                            )}
                            <button
                              onClick={() => updateGatewayState(agent.provider, { expanded: false })}
                              className="px-2 py-1.5 text-[11px] text-text-muted hover:text-text-base transition-colors"
                            >
                              Cancel
                            </button>
                            {gs.saveStatus === "saved" && (
                              <span className="text-[11px] text-success">Saved</span>
                            )}
                            {gs.saveStatus === "error" && (
                              <span className="text-[11px] text-danger">Failed to save</span>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  );
                })()}

                {state.saveStatus === "saved" && !state.editing && (
                  <div className="px-3 pb-3">
                    <span className="text-[11px] text-success">Key saved to keychain</span>
                  </div>
                )}
                {state.saveStatus === "error" && !state.editing && (
                  <div className="px-3 pb-3">
                    <span className="text-[11px] text-danger">Failed to save key</span>
                  </div>
                )}

                {state.editing && (
                  <div className="border-t border-border-strong/30 px-3 py-3">
                    <div className="flex flex-col gap-2">
                      <div className="flex items-center gap-2">
                        <div className="flex-1 relative">
                          <input
                            type={state.revealed ? "text" : "password"}
                            value={state.inputValue}
                            onChange={(e) => updateState(agent.provider, { inputValue: e.target.value })}
                            onKeyDown={(e) => {
                              if (e.key === "Escape") updateState(agent.provider, { editing: false, inputValue: "", inputValue2: "", revealed: false });
                            }}
                            placeholder={agent.placeholder}
                            autoFocus
                            className="w-full text-[13px] text-text-base font-mono bg-bg-base border border-border-strong/40 rounded px-3 py-2 pr-9 focus:outline-none focus:border-brand transition-colors"
                          />
                          <button
                            onClick={() => updateState(agent.provider, { revealed: !state.revealed })}
                            className="absolute right-2.5 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors"
                            title={state.revealed ? "Hide" : "Reveal"}
                            type="button"
                          >
                            {state.revealed ? <EyeOff size={13} /> : <Eye size={13} />}
                          </button>
                        </div>
                        {!agent.accountIdPlaceholder && (
                          <>
                            <button
                              onClick={() => saveKey(agent.provider)}
                              disabled={!state.inputValue.trim() || state.saveStatus === "saving"}
                              className="px-3 py-2 text-[12px] font-medium rounded bg-brand text-white hover:bg-brand-active disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                            >
                              {state.saveStatus === "saving" ? "Saving..." : "Save"}
                            </button>
                            <button
                              onClick={() => updateState(agent.provider, { editing: false, inputValue: "", inputValue2: "", revealed: false })}
                              className="px-2 py-2 text-[12px] text-text-muted hover:text-text-base transition-colors"
                            >
                              Cancel
                            </button>
                          </>
                        )}
                      </div>
                      {agent.accountIdPlaceholder && (
                        <div className="flex items-center gap-2">
                          <input
                            type="text"
                            value={state.inputValue2}
                            onChange={(e) => updateState(agent.provider, { inputValue2: e.target.value })}
                            onKeyDown={(e) => {
                              if (e.key === "Escape") updateState(agent.provider, { editing: false, inputValue: "", inputValue2: "", revealed: false });
                            }}
                            placeholder={agent.accountIdPlaceholder}
                            className="flex-1 text-[13px] text-text-base font-mono bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand transition-colors"
                          />
                          <button
                            onClick={() => saveKey(agent.provider)}
                            disabled={!state.inputValue.trim() || !state.inputValue2.trim() || state.saveStatus === "saving"}
                            className="px-3 py-2 text-[12px] font-medium rounded bg-brand text-white hover:bg-brand-active disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                          >
                            {state.saveStatus === "saving" ? "Saving..." : "Save"}
                          </button>
                          <button
                            onClick={() => updateState(agent.provider, { editing: false, inputValue: "", inputValue2: "", revealed: false })}
                            className="px-2 py-2 text-[12px] text-text-muted hover:text-text-base transition-colors"
                          >
                            Cancel
                          </button>
                        </div>
                      )}
                    </div>
                    <p className="text-[11px] text-text-muted mt-2 leading-relaxed">
                      Your credentials are stored in the OS keychain and never written to disk.
                    </p>
                  </div>
                )}
              </div>
            </section>
          );
        })}
      </div>
    </div>
  );
}
