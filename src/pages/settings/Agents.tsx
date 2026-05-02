import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Eye, EyeOff, Key, Trash2 } from "lucide-react";
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
}

const AGENTS: AgentDefinition[] = [
  {
    id: "claude",
    label: "Claude",
    description: "Anthropic's Claude models. Used for file generation, insight generation, recommendations, and the AI Playground.",
    provider: "anthropic",
    providerLabel: "Anthropic",
    placeholder: "sk-ant-...",
  },
];

type SaveStatus = "idle" | "saving" | "saved" | "error";

interface KeyState {
  stored: boolean;
  inputValue: string;
  editing: boolean;
  revealed: boolean;
  saveStatus: SaveStatus;
}

function defaultKeyState(): KeyState {
  return { stored: false, inputValue: "", editing: false, revealed: false, saveStatus: "idle" };
}

export default function SettingsAgents() {
  const [keyStates, setKeyStates] = useState<Record<string, KeyState>>({});

  useEffect(() => {
    loadKeyStatus();
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
  };

  const updateState = (provider: string, patch: Partial<KeyState>) => {
    setKeyStates((prev) => ({
      ...prev,
      [provider]: { ...(prev[provider] ?? defaultKeyState()), ...patch },
    }));
  };

  const saveKey = async (provider: string) => {
    const state = keyStates[provider];
    if (!state || !state.inputValue.trim()) return;
    updateState(provider, { saveStatus: "saving" });
    try {
      await invoke("save_api_key", { provider, key: state.inputValue.trim() });
      updateState(provider, {
        stored: true,
        inputValue: "",
        editing: false,
        revealed: false,
        saveStatus: "saved",
      });
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
        editing: false,
        revealed: false,
        saveStatus: "idle",
      });
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
                        onClick={() => updateState(agent.provider, { editing: true, inputValue: "", revealed: false })}
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
                    <div className="flex items-center gap-2">
                      <div className="flex-1 relative">
                        <input
                          type={state.revealed ? "text" : "password"}
                          value={state.inputValue}
                          onChange={(e) => updateState(agent.provider, { inputValue: e.target.value })}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && state.inputValue.trim()) saveKey(agent.provider);
                            if (e.key === "Escape") updateState(agent.provider, { editing: false, inputValue: "", revealed: false });
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
                      <button
                        onClick={() => saveKey(agent.provider)}
                        disabled={!state.inputValue.trim() || state.saveStatus === "saving"}
                        className="px-3 py-2 text-[12px] font-medium rounded bg-brand text-white hover:bg-brand-active disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                      >
                        {state.saveStatus === "saving" ? "Saving..." : "Save"}
                      </button>
                      <button
                        onClick={() => updateState(agent.provider, { editing: false, inputValue: "", revealed: false })}
                        className="px-2 py-2 text-[12px] text-text-muted hover:text-text-base transition-colors"
                      >
                        Cancel
                      </button>
                    </div>
                    <p className="text-[11px] text-text-muted mt-2 leading-relaxed">
                      Your key is stored in the OS keychain and never written to disk.
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
