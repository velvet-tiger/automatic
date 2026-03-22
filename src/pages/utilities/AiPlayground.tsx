import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Send, Trash2, AlertCircle, ChevronDown, ChevronUp, Bot, Check, FolderOpen } from "lucide-react";
import { flag } from "../../lib/flags";

// ── Types ─────────────────────────────────────────────────────────────────────

interface AiMessage {
  role: "user" | "assistant";
  content: string;
}

/** Static fallback list used when the API key is absent or the request fails. */
const FALLBACK_MODELS = [
  "claude-sonnet-4-5",
  "claude-opus-4-5",
  "claude-haiku-4-5",
  "claude-3-5-sonnet-20241022",
  "claude-3-5-haiku-20241022",
  "claude-3-opus-20240229",
];

// ── Shared dropdown hook ──────────────────────────────────────────────────────

function useDropdown() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  return { open, setOpen, ref };
}

// ── ModelPicker ───────────────────────────────────────────────────────────────

function ModelPicker({
  value,
  onChange,
  models,
}: {
  value: string;
  onChange: (v: string) => void;
  models: string[];
}) {
  const { open, setOpen, ref } = useDropdown();

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[12px] text-text-base bg-bg-input border border-border-strong/40 hover:border-border-strong/70 transition-colors"
      >
        {value}
        <ChevronDown size={11} className={`text-text-muted transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute left-0 top-full mt-1 z-50 min-w-[220px] max-h-72 overflow-y-auto rounded-md bg-bg-input border border-border-strong/40 shadow-lg custom-scrollbar">
          {models.map((m) => (
            <button
              key={m}
              onClick={() => { onChange(m); setOpen(false); }}
              className={`w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-left transition-colors ${
                m === value
                  ? "text-text-base bg-bg-sidebar"
                  : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
              }`}
            >
              <Check size={11} className={m === value ? "opacity-100 text-brand" : "opacity-0"} />
              {m}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// ── ProjectPicker ─────────────────────────────────────────────────────────────

interface ProjectOption {
  name: string;
  directory: string;
}

function ProjectPicker({
  projects,
  value,
  onChange,
}: {
  projects: ProjectOption[];
  value: string;
  onChange: (name: string, directory: string) => void;
}) {
  const { open, setOpen, ref } = useDropdown();
  const selected = projects.find((p) => p.name === value);

  // Truncate a long path to the last N path segments for display.
  const shortenPath = (dir: string, segments = 3) => {
    const parts = dir.replace(/\\/g, "/").split("/").filter(Boolean);
    if (parts.length <= segments) return dir;
    return "…/" + parts.slice(-segments).join("/");
  };

  if (projects.length === 0) {
    return (
      <span className="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[12px] text-text-muted bg-bg-input border border-border-strong/40 italic">
        <FolderOpen size={11} />
        No projects
      </span>
    );
  }

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[12px] text-text-base bg-bg-input border border-border-strong/40 hover:border-border-strong/70 transition-colors max-w-[200px]"
      >
        <FolderOpen size={11} className="text-text-muted shrink-0" />
        <span className="truncate">{selected?.name ?? "Select project…"}</span>
        <ChevronDown size={11} className={`text-text-muted transition-transform shrink-0 ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute right-0 top-full mt-1 z-50 min-w-[260px] max-w-[340px] rounded-md bg-bg-input border border-border-strong/40 shadow-lg overflow-hidden">
          {projects.map((p) => (
            <button
              key={p.name}
              onClick={() => { onChange(p.name, p.directory); setOpen(false); }}
              className={`w-full flex items-start gap-2 px-3 py-2 text-left transition-colors ${
                p.name === value
                  ? "text-text-base bg-bg-sidebar"
                  : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
              }`}
            >
              <Check size={11} className={`mt-0.5 shrink-0 ${p.name === value ? "opacity-100 text-brand" : "opacity-0"}`} />
              <span className="flex flex-col min-w-0">
                <span className="text-[12px] font-medium truncate">{p.name}</span>
                <span className="text-[10px] text-text-muted/70 truncate font-mono mt-0.5">
                  {shortenPath(p.directory)}
                </span>
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Component ─────────────────────────────────────────────────────────────────

export default function AiPlayground() {
  const toolsEnabled = flag("ai_toolset");

  const [messages, setMessages] = useState<AiMessage[]>([]);
  const [input, setInput] = useState("");
  const [system, setSystem] = useState("");
  const [showSystem, setShowSystem] = useState(false);
  const [models, setModels] = useState<string[]>(FALLBACK_MODELS);
  const [model, setModel] = useState(FALLBACK_MODELS[0]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [workingDir, setWorkingDir] = useState<string>("");
  const [projects, setProjects] = useState<Array<{ name: string; directory: string }>>([]);
  const [selectedProject, setSelectedProject] = useState<string>("");
  const [hasAnthropicKey, setHasAnthropicKey] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  // ── Check API key availability and fetch available models ─────────────────

  useEffect(() => {
    // Check whether a key is resolvable (env var or keychain).
    invoke<boolean>("has_ai_key")
      .then(setHasAnthropicKey)
      .catch(() => setHasAnthropicKey(false));

    invoke<string[]>("ai_list_models")
      .then((fetched) => {
        if (fetched.length > 0) {
          setModels(fetched);
          // Keep the selected model if it's in the fetched list; otherwise
          // default to the first result (newest model).
          setModel((prev) => (fetched.includes(prev) ? prev : fetched[0]!));
        }
      })
      .catch(() => {
        // No key configured yet, or network error — stay on the fallback list.
      });
  }, []);

  // ── Load projects for the working-dir picker ───────────────────────────────

  useEffect(() => {
    if (!toolsEnabled) return;
    invoke<string[]>("get_projects")
      .then((names) =>
        Promise.all(
          names.map((name) =>
            invoke<string>("read_project", { name }).then((raw) => {
              const p = JSON.parse(raw);
              return { name, directory: p?.directory ?? "" };
            })
          )
        )
      )
      .then((all) => {
        const withDir = all.filter((p) => p.directory);
        setProjects(withDir);
        // Default to the first project that has a directory — never home.
        if (withDir.length > 0 && !selectedProject) {
          setSelectedProject(withDir[0]!.name);
          setWorkingDir(withDir[0]!.directory);
        }
      })
      .catch(() => {});
  }, [toolsEnabled]);

  // ── Scroll to bottom on new messages ──────────────────────────────────────

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading]);

  // ── Send message ───────────────────────────────────────────────────────────

  const send = async () => {
    const text = input.trim();
    if (!text || loading) return;

    const userMsg: AiMessage = { role: "user", content: text };
    const nextMessages = [...messages, userMsg];
    setMessages(nextMessages);
    setInput("");
    setError(null);
    setLoading(true);

    try {
      let response: string;
      if (toolsEnabled) {
        if (!workingDir) {
          setError("Select a project directory before using file tools.");
          setLoading(false);
          setMessages((prev) => prev.slice(0, -1)); // remove the optimistic user msg
          setInput(text);
          return;
        }
        response = await invoke("ai_chat_with_tools", {
          messages: nextMessages,
          apiKey: null,
          model: model || null,
          system: system.trim() || null,
          maxTokens: null,
          workingDir,
        });
      } else {
        response = await invoke("ai_chat", {
          messages: nextMessages,
          apiKey: null,
          model: model || null,
          system: system.trim() || null,
          maxTokens: null,
        });
      }
      setMessages((prev) => [...prev, { role: "assistant", content: response }]);
    } catch (err: any) {
      setError(typeof err === "string" ? err : JSON.stringify(err));
    } finally {
      setLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      send();
    }
  };

  const clearConversation = () => {
    setMessages([]);
    setError(null);
  };

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full">

      {/* ── Header bar ── */}
      <div className="border-b border-border-strong/40 px-5 py-3 flex items-center gap-3 bg-bg-base shrink-0">
        <Bot size={15} className="text-brand" />
        <span className="text-[13px] font-medium text-text-base">AI Playground</span>

        <ModelPicker value={model} onChange={setModel} models={models} />

        {/* Tools badge */}
        {toolsEnabled && (
          <span className="flex items-center gap-1 px-2 py-0.5 rounded text-[11px] bg-brand/15 text-brand border border-brand/30">
            <FolderOpen size={10} />
            read_file
          </span>
        )}

        <div className="ml-auto flex items-center gap-2">
          {/* Project / working-dir picker (visible when tools are on) */}
          {toolsEnabled && (
            <ProjectPicker
              projects={projects}
              value={selectedProject}
              onChange={(name, directory) => {
                setSelectedProject(name);
                setWorkingDir(directory);
              }}
            />
          )}
          {messages.length > 0 && (
            <button
              onClick={clearConversation}
              className="flex items-center gap-1 text-[11px] text-text-muted hover:text-red-400 transition-colors"
            >
              <Trash2 size={12} />
              Clear
            </button>
          )}
        </div>
      </div>

      {/* ── System prompt panel ── */}
      <div className="border-b border-border-strong/40 bg-bg-base shrink-0">
        <button
          onClick={() => setShowSystem((v) => !v)}
          className="w-full flex items-center gap-2 px-5 py-2 text-[12px] text-text-muted hover:text-text-base transition-colors"
        >
          {showSystem ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
          System prompt {system.trim() ? `(${system.trim().slice(0, 40)}${system.length > 40 ? "…" : ""})` : "(none)"}
        </button>
        {showSystem && (
          <div className="px-5 pb-3">
            <textarea
              value={system}
              onChange={(e) => setSystem(e.target.value)}
              placeholder="You are a helpful assistant..."
              rows={3}
              className="w-full bg-bg-input border border-border-strong/40 rounded-md text-[12px] text-text-base px-3 py-2 resize-none focus:outline-none focus:ring-1 focus:ring-brand placeholder:text-text-muted/50 custom-scrollbar"
            />
          </div>
        )}
      </div>

      {/* ── Conversation ── */}
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4 custom-scrollbar">
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-center text-text-muted select-none">
            <Bot size={32} className="mb-3 opacity-30" />
            <p className="text-[13px] font-medium mb-1">AI Playground</p>
            <p className="text-[12px] opacity-70">Send a message to test the Anthropic integration.</p>
            <p className="text-[11px] mt-3 opacity-50">Cmd+Enter to send</p>
          </div>
        )}

        {messages.map((msg, i) => (
          <div
            key={i}
            className={`flex flex-col gap-1 ${msg.role === "user" ? "items-end" : "items-start"}`}
          >
            <span className="text-[10px] text-text-muted uppercase tracking-wider px-1">
              {msg.role === "user" ? "You" : "Assistant"}
            </span>
            <div
              className={`max-w-[80%] rounded-lg px-3.5 py-2.5 text-[13px] leading-relaxed whitespace-pre-wrap break-words ${
                msg.role === "user"
                  ? "bg-brand text-white"
                  : "bg-bg-input text-text-base border border-border-strong/40"
              }`}
            >
              {msg.content}
            </div>
          </div>
        ))}

        {loading && (
          <div className="flex flex-col gap-1 items-start">
            <span className="text-[10px] text-text-muted uppercase tracking-wider px-1">Assistant</span>
            <div className="bg-bg-input border border-border-strong/40 rounded-lg px-3.5 py-2.5">
              <span className="flex gap-1">
                {[0, 1, 2].map((i) => (
                  <span
                    key={i}
                    className="w-1.5 h-1.5 rounded-full bg-text-muted animate-bounce"
                    style={{ animationDelay: `${i * 0.15}s` }}
                  />
                ))}
              </span>
            </div>
          </div>
        )}

        {error && (
          <div className="flex items-start gap-2 rounded-lg border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-[12px] text-red-400">
            <AlertCircle size={14} className="mt-0.5 shrink-0" />
            <span className="break-words">{error}</span>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* ── Input area ── */}
      <div className="border-t border-border-strong/40 bg-bg-base px-4 py-3 shrink-0">
        <div className="flex gap-2 items-end">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type a message… (Cmd+Enter to send)"
            rows={3}
            className="flex-1 bg-bg-input border border-border-strong/40 rounded-lg text-[13px] text-text-base px-3 py-2 resize-none focus:outline-none focus:ring-1 focus:ring-brand placeholder:text-text-muted/50 custom-scrollbar"
          />
          <span className="relative group/keytip shrink-0">
            <button
              onClick={send}
              disabled={!input.trim() || loading || !hasAnthropicKey}
              className="flex items-center justify-center w-9 h-9 mb-0.5 rounded-lg bg-brand hover:bg-brand-hover text-white transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              <Send size={15} />
            </button>
            {!hasAnthropicKey && (
              <span className="pointer-events-none absolute bottom-full right-0 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                Add your Anthropic API key to access
              </span>
            )}
          </span>
        </div>
      </div>
    </div>
  );
}
