import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Plus,
  X,
  Server,
  Check,
  Trash2,
  Terminal,
  Variable,
  Globe,
  ToggleLeft,
  ToggleRight,
  AlertTriangle,
} from "lucide-react";
import { ICONS } from "./icons";

type TransportType = "stdio" | "http" | "sse";

interface McpServerConfig {
  type: TransportType;
  // stdio fields
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  // http/sse fields
  url?: string;
  headers?: Record<string, string>;
  oauth?: {
    clientId?: string;
    clientSecret?: string;
    scope?: string;
    callbackPort?: number;
  };
  // common
  enabled?: boolean;
  timeout?: number;
}

function emptyConfig(): McpServerConfig {
  return {
    type: "stdio",
    command: "",
    args: [],
    env: {},
    enabled: true,
  };
}

/** Normalize a loaded config so all optional fields have sensible defaults for the UI. */
function normalizeConfig(data: Partial<McpServerConfig> & { oauth?: any }): McpServerConfig {
  let type: TransportType = data.type || "stdio";
  if (!data.type && data.url && !data.command) {
    type = "http";
  }
  
  let oauth;
  if (data.oauth && typeof data.oauth === 'object') {
    oauth = {
      clientId: data.oauth.clientId || "",
      clientSecret: data.oauth.clientSecret || "",
      scope: data.oauth.scope || "",
      callbackPort: data.oauth.callbackPort || undefined,
    };
  }

  return {
    type,
    command: data.command || "",
    args: data.args || [],
    env: data.env || {},
    cwd: data.cwd || "",
    url: data.url || "",
    headers: data.headers || {},
    oauth,
    enabled: data.enabled !== false,
    timeout: data.timeout,
  };
}

/** Strip empty optional fields before saving. */
function cleanConfig(config: McpServerConfig): Record<string, unknown> {
  const out: Record<string, unknown> = { type: config.type };

  if (config.type === "stdio") {
    if (config.command) out.command = config.command;
    if (config.args && config.args.length > 0) out.args = config.args;
    if (config.env && Object.keys(config.env).length > 0) out.env = config.env;
    if (config.cwd) out.cwd = config.cwd;
  } else {
    if (config.url) out.url = config.url;
    if (config.headers && Object.keys(config.headers).length > 0) out.headers = config.headers;
    
    if (config.oauth) {
      const cleanOauth: Record<string, unknown> = {};
      if (config.oauth.clientId) cleanOauth.clientId = config.oauth.clientId;
      if (config.oauth.clientSecret) cleanOauth.clientSecret = config.oauth.clientSecret;
      if (config.oauth.scope) cleanOauth.scope = config.oauth.scope;
      if (config.oauth.callbackPort) cleanOauth.callbackPort = config.oauth.callbackPort;
      
      if (Object.keys(cleanOauth).length > 0) {
        out.oauth = cleanOauth;
      }
    }
  }

  if (config.enabled === false) out.enabled = false;
  if (config.timeout && config.timeout > 0) out.timeout = config.timeout;

  return out;
}

// ── Reusable field components ──────────────────────────────────────────────

const inputClass =
  "w-full bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-2 text-[13px] text-[#F8F8FA] placeholder-[#C8CAD0]/40 outline-none font-mono transition-colors";

const smallInputClass =
  "flex-1 bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-1.5 text-[13px] text-[#F8F8FA] placeholder-[#C8CAD0]/40 outline-none font-mono transition-colors";

const addBtnClass =
  "px-3 py-1.5 bg-[#2D2E36] hover:bg-[#33353A] text-[#F8F8FA] text-[12px] font-medium rounded border border-[#3A3B42] transition-colors disabled:opacity-30 disabled:cursor-not-allowed";

function KvList({
  entries,
  onRemove,
  colorKey,
}: {
  entries: [string, string][];
  onRemove: (key: string) => void;
  colorKey?: boolean;
}) {
  if (entries.length === 0) return null;
  return (
    <ul className="space-y-1 mb-2">
      {entries.map(([key, val]) => (
        <li
          key={key}
          className="group flex items-center justify-between px-3 py-1.5 bg-[#1A1A1E] rounded-md border border-[#33353A] text-[13px] font-mono"
        >
          <span className="truncate">
            <span className={colorKey ? "text-[#8C98F2]" : "text-[#F8F8FA]"}>{key}</span>
            <span className="text-[#C8CAD0] mx-1">=</span>
            <span className="text-[#F8F8FA]">{val}</span>
          </span>
          <button
            onClick={() => onRemove(key)}
            className="text-[#C8CAD0] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all flex-shrink-0 ml-2"
          >
            <Trash2 size={12} />
          </button>
        </li>
      ))}
    </ul>
  );
}

// ── Main component ─────────────────────────────────────────────────────────

interface Project {
  name: string;
  agents: string[];
  mcp_servers: string[];
}

export default function McpServers() {
  const [servers, setServers] = useState<string[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [config, setConfig] = useState<McpServerConfig | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [opencodeWarning, setOpencodeWarning] = useState<string[]>([]);

  // Inline add state
  const [newArg, setNewArg] = useState("");
  const [newEnvKey, setNewEnvKey] = useState("");
  const [newEnvVal, setNewEnvVal] = useState("");
  const [newHeaderKey, setNewHeaderKey] = useState("");
  const [newHeaderVal, setNewHeaderVal] = useState("");

  useEffect(() => {
    loadServers();
    checkOpencodeProjects();
  }, []);

  const loadServers = async () => {
    try {
      const result: string[] = await invoke("list_mcp_server_configs");
      setServers(result.sort());
      setError(null);
    } catch (err: any) {
      setError(`Failed to load servers: ${err}`);
    }
  };

  const checkOpencodeProjects = async () => {
    try {
      const projectNames: string[] = await invoke("list_projects");
      const affectedProjects: string[] = [];

      for (const name of projectNames) {
        const raw: string = await invoke("read_project", { name });
        const project: Project = JSON.parse(raw);
        
        // Check if project uses OpenCode and has MCP servers configured
        if (project.agents.includes("opencode") && project.mcp_servers.length > 0) {
          affectedProjects.push(project.name);
        }
      }

      setOpencodeWarning(affectedProjects);
    } catch (err: any) {
      // Silently fail - warning is non-critical
      console.error("Failed to check OpenCode projects:", err);
    }
  };

  const selectServer = async (name: string) => {
    try {
      const raw: string = await invoke("read_mcp_server_config", { name });
      const data = JSON.parse(raw);
      setSelectedName(name);
      setConfig(normalizeConfig(data));
      setDirty(false);
      setIsCreating(false);
      setError(null);
      resetInlineState();
    } catch (err: any) {
      setError(`Failed to read server: ${err}`);
    }
  };

  const resetInlineState = () => {
    setNewArg("");
    setNewEnvKey("");
    setNewEnvVal("");
    setNewHeaderKey("");
    setNewHeaderVal("");
  };

  const updateConfig = (patch: Partial<McpServerConfig>) => {
    if (!config) return;
    setConfig({ ...config, ...patch });
    setDirty(true);
  };

  const handleSave = async () => {
    if (!config) return;
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;
    try {
      await invoke("save_mcp_server_config", {
        name,
        data: JSON.stringify(cleanConfig(config)),
      });
      setDirty(false);
      setSelectedName(name);
      if (isCreating) {
        setIsCreating(false);
        await loadServers();
      }
      setError(null);
    } catch (err: any) {
      setError(`Failed to save server: ${err}`);
    }
  };

  const handleDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invoke("delete_mcp_server_config", { name });
      if (selectedName === name) {
        setSelectedName(null);
        setConfig(null);
        setDirty(false);
      }
      await loadServers();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete server: ${err}`);
    }
  };

  const startCreate = () => {
    setSelectedName(null);
    setConfig(emptyConfig());
    setDirty(true);
    setIsCreating(true);
    setNewName("");
    resetInlineState();
  };

  const addArg = () => {
    if (!config || !newArg) return;
    updateConfig({ args: [...(config.args || []), newArg] });
    setNewArg("");
  };

  const removeArg = (idx: number) => {
    if (!config) return;
    updateConfig({ args: (config.args || []).filter((_, i) => i !== idx) });
  };

  const addEnv = () => {
    if (!config || !newEnvKey.trim()) return;
    updateConfig({ env: { ...(config.env || {}), [newEnvKey.trim()]: newEnvVal } });
    setNewEnvKey("");
    setNewEnvVal("");
  };

  const removeEnv = (key: string) => {
    if (!config) return;
    const { [key]: _, ...rest } = config.env || {};
    updateConfig({ env: rest });
  };

  const addHeader = () => {
    if (!config || !newHeaderKey.trim()) return;
    updateConfig({ headers: { ...(config.headers || {}), [newHeaderKey.trim()]: newHeaderVal } });
    setNewHeaderKey("");
    setNewHeaderVal("");
  };

  const removeHeader = (key: string) => {
    if (!config) return;
    const { [key]: _, ...rest } = config.headers || {};
    updateConfig({ headers: rest });
  };

  const setTransport = (type: TransportType) => {
    if (!config) return;
    setConfig({
      ...config,
      type,
      ...(type === "stdio"
        ? { command: config.command || "", args: config.args || [], cwd: config.cwd || "" }
        : { url: config.url || "", headers: config.headers || {} }),
    });
    setDirty(true);
  };

  const isStdio = config?.type === "stdio";

  return (
    <div className="flex h-full w-full bg-[#222327]">
      {/* Left sidebar */}
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50">
        <div className="h-11 px-4 border-b border-[#33353A] flex justify-between items-center bg-[#222327]/30">
          <span className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">
            MCP Servers
          </span>
          <div className="flex items-center gap-1">
            <button
              onClick={startCreate}
              className="text-[#C8CAD0] hover:text-[#F8F8FA] transition-colors p-1 hover:bg-[#2D2E36] rounded"
              title="Add MCP Server"
            >
              <Plus size={14} />
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {servers.length === 0 && !isCreating ? (
            <div className="px-4 py-6 text-center">
              <p className="text-[13px] text-[#C8CAD0]">No servers configured.</p>
            </div>
          ) : (
            <ul className="space-y-1 px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-[#2D2E36]">
                  <div className={ICONS.mcp.iconBox}>
                    <Server size={15} className={ICONS.mcp.iconColor} />
                  </div>
                  <span className="text-[13px] text-[#F8F8FA] italic">New Server...</span>
                </li>
              )}
              {servers.map((name) => {
                const isActive = selectedName === name && !isCreating;
                return (
                  <li key={name} className="group relative">
                    <button
                      onClick={() => selectServer(name)}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isActive
                          ? "bg-[#2D2E36] text-[#F8F8FA]"
                          : "text-[#C8CAD0] hover:bg-[#2D2E36]/60 hover:text-[#F8F8FA]"
                      }`}
                    >
                      <div className={ICONS.mcp.iconBox}>
                        <Server size={15} className={ICONS.mcp.iconColor} />
                      </div>
                      <span className={`flex-1 text-[13px] font-medium truncate ${isActive ? "text-[#F8F8FA]" : "text-[#E8E9ED]"}`}>
                        {name}
                      </span>
                    </button>
                    <button
                      onClick={(e) => handleDelete(name, e)}
                      className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-[#C8CAD0] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 hover:bg-[#33353A] rounded transition-all"
                      title="Delete Server"
                    >
                      <X size={12} />
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      {/* Right area */}
      <div className="flex-1 flex flex-col min-w-0 bg-[#222327]">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between">
            {error}
            <button onClick={() => setError(null)}>
              <X size={14} />
            </button>
          </div>
        )}

        {opencodeWarning.length > 0 && (
          <div className="bg-amber-500/10 text-amber-400 p-3 text-[13px] border-b border-amber-500/20 flex items-start gap-3">
            <AlertTriangle size={16} className="flex-shrink-0 mt-0.5" />
            <div className="flex-1">
              <div className="font-medium mb-1">MCP Access Issue with OpenCode</div>
              <div className="text-[12px] text-amber-300/90 leading-relaxed">
                The following project{opencodeWarning.length > 1 ? 's are' : ' is'} using OpenCode with MCP servers configured: <span className="font-medium">{opencodeWarning.join(", ")}</span>. 
                MCP server access is currently broken in Claude when using OpenCode. 
                MCP servers will be written to opencode.json but may not function correctly until this issue is resolved.
              </div>
            </div>
            <button onClick={() => setOpencodeWarning([])} className="text-amber-300 hover:text-amber-200">
              <X size={14} />
            </button>
          </div>
        )}

        {config ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="h-11 px-6 border-b border-[#33353A] flex justify-between items-center">
              <div className="flex items-center gap-3">
                <Server size={14} className={ICONS.mcp.iconColor} />
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="server-name (no spaces/slashes)"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-[#F8F8FA] placeholder-[#C8CAD0]/50 w-64"
                  />
                ) : (
                  <h3 className="text-[14px] font-medium text-[#F8F8FA]">{selectedName}</h3>
                )}
              </div>

              <div className="flex items-center gap-2">
                {dirty && (
                  <button
                    onClick={handleSave}
                    disabled={isCreating && !newName.trim()}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                  >
                    <Check size={12} /> Save
                  </button>
                )}
              </div>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="max-w-2xl space-y-8">
                {/* Transport Type + Enabled */}
                <section>
                  <div className="flex items-center justify-between mb-2">
                    <label className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">
                      Type
                    </label>
                    <button
                      onClick={() => updateConfig({ enabled: !config.enabled })}
                      className={`flex items-center gap-1.5 text-[12px] transition-colors ${
                        config.enabled !== false ? "text-[#4ADE80]" : "text-[#C8CAD0]"
                      }`}
                      title={config.enabled !== false ? "Enabled — click to disable" : "Disabled — click to enable"}
                    >
                      {config.enabled !== false ? <ToggleRight size={16} /> : <ToggleLeft size={16} />}
                      {config.enabled !== false ? "Enabled" : "Disabled"}
                    </button>
                  </div>
                  <div className="flex gap-1.5">
                    <button
                      onClick={() => setTransport("stdio")}
                      className={`px-3 py-1.5 rounded text-[12px] font-medium transition-colors border ${
                        isStdio
                          ? "bg-[#5E6AD2] border-[#5E6AD2] text-white"
                          : "bg-[#1A1A1E] border-[#33353A] text-[#C8CAD0] hover:border-[#44474F] hover:text-[#F8F8FA]"
                      }`}
                    >
                      Local
                    </button>
                    <button
                      onClick={() => setTransport(config.type === "sse" ? "sse" : "http")}
                      className={`px-3 py-1.5 rounded text-[12px] font-medium transition-colors border ${
                        !isStdio
                          ? "bg-[#5E6AD2] border-[#5E6AD2] text-white"
                          : "bg-[#1A1A1E] border-[#33353A] text-[#C8CAD0] hover:border-[#44474F] hover:text-[#F8F8FA]"
                      }`}
                    >
                      Remote
                    </button>
                  </div>
                  <p className="mt-1.5 text-[11px] text-[#C8CAD0]">
                    {isStdio
                      ? "Launches a local process and communicates via stdin/stdout."
                      : "Connects to a remote MCP server over HTTP."}
                  </p>
                </section>

                {/* HTTP vs SSE sub-option for remote */}
                {!isStdio && (
                  <section>
                    <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                      Transport Protocol
                    </label>
                    <div className="flex gap-1.5">
                      <button
                        onClick={() => setTransport("http")}
                        className={`px-3 py-1.5 rounded text-[12px] font-medium transition-colors border ${
                          config.type === "http"
                            ? "bg-[#2D2E36] border-[#44474F] text-[#F8F8FA]"
                            : "bg-[#1A1A1E] border-[#33353A] text-[#C8CAD0] hover:border-[#44474F] hover:text-[#F8F8FA]"
                        }`}
                      >
                        Streamable HTTP
                      </button>
                      <button
                        onClick={() => setTransport("sse")}
                        className={`px-3 py-1.5 rounded text-[12px] font-medium transition-colors border ${
                          config.type === "sse"
                            ? "bg-[#2D2E36] border-[#44474F] text-[#F8F8FA]"
                            : "bg-[#1A1A1E] border-[#33353A] text-[#C8CAD0] hover:border-[#44474F] hover:text-[#F8F8FA]"
                        }`}
                      >
                        SSE (legacy)
                      </button>
                    </div>
                  </section>
                )}

                {/* ── stdio fields ────────────────────────────────────────── */}
                {isStdio && (
                  <>
                    <section>
                      <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                        <span className="flex items-center gap-1.5">
                          <Terminal size={12} /> Command
                        </span>
                      </label>
                      <input
                        type="text"
                        value={config.command || ""}
                        onChange={(e) => updateConfig({ command: e.target.value })}
                        placeholder="e.g. npx, node, /usr/local/bin/mcp-server"
                        className={inputClass}
                      />
                    </section>

                    <section>
                      <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                        Arguments
                      </label>
                      {(config.args || []).length > 0 && (
                        <ul className="space-y-1 mb-2">
                          {(config.args || []).map((arg, i) => (
                            <li
                              key={i}
                              className="group flex items-center justify-between px-3 py-1.5 bg-[#1A1A1E] rounded-md border border-[#33353A] text-[13px] text-[#F8F8FA] font-mono"
                            >
                              <span className="truncate">{arg}</span>
                              <button
                                onClick={() => removeArg(i)}
                                className="text-[#C8CAD0] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all flex-shrink-0 ml-2"
                              >
                                <Trash2 size={12} />
                              </button>
                            </li>
                          ))}
                        </ul>
                      )}
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={newArg}
                          onChange={(e) => setNewArg(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && newArg) {
                              e.preventDefault();
                              addArg();
                            }
                          }}
                          placeholder="Add argument..."
                          className={smallInputClass}
                        />
                        <button onClick={addArg} disabled={!newArg} className={addBtnClass}>
                          Add
                        </button>
                      </div>
                    </section>

                    <section>
                      <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                        Working Directory
                      </label>
                      <input
                        type="text"
                        value={config.cwd || ""}
                        onChange={(e) => updateConfig({ cwd: e.target.value })}
                        placeholder="Optional — defaults to system default"
                        className={inputClass}
                      />
                    </section>

                    <section>
                      <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                        <span className="flex items-center gap-1.5">
                          <Variable size={12} /> Environment Variables
                        </span>
                      </label>
                      <KvList
                        entries={Object.entries(config.env || {})}
                        onRemove={removeEnv}
                        colorKey
                      />
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={newEnvKey}
                          onChange={(e) => setNewEnvKey(e.target.value)}
                          placeholder="KEY"
                          className="w-40 bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-1.5 text-[13px] text-[#F8F8FA] placeholder-[#C8CAD0]/40 outline-none font-mono transition-colors"
                        />
                        <input
                          type="text"
                          value={newEnvVal}
                          onChange={(e) => setNewEnvVal(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && newEnvKey.trim()) {
                              e.preventDefault();
                              addEnv();
                            }
                          }}
                          placeholder="value"
                          className={smallInputClass}
                        />
                        <button onClick={addEnv} disabled={!newEnvKey.trim()} className={addBtnClass}>
                          Add
                        </button>
                      </div>
                    </section>
                  </>
                )}

                {/* ── http/sse fields ─────────────────────────────────────── */}
                {!isStdio && (
                  <>
                    <section>
                      <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                        <span className="flex items-center gap-1.5">
                          <Globe size={12} /> URL
                        </span>
                      </label>
                      <input
                        type="text"
                        value={config.url || ""}
                        onChange={(e) => updateConfig({ url: e.target.value })}
                        placeholder={
                          config.type === "sse"
                            ? "https://example.com/sse"
                            : "https://example.com/mcp"
                        }
                        className={inputClass}
                      />
                    </section>

                    <section>
                      <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                        HTTP Headers
                      </label>
                      <KvList
                        entries={Object.entries(config.headers || {})}
                        onRemove={removeHeader}
                        colorKey
                      />
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={newHeaderKey}
                          onChange={(e) => setNewHeaderKey(e.target.value)}
                          placeholder="Header-Name"
                          className="w-48 bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-1.5 text-[13px] text-[#F8F8FA] placeholder-[#C8CAD0]/40 outline-none font-mono transition-colors"
                        />
                        <input
                          type="text"
                          value={newHeaderVal}
                          onChange={(e) => setNewHeaderVal(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && newHeaderKey.trim()) {
                              e.preventDefault();
                              addHeader();
                            }
                          }}
                          placeholder="value"
                          className={smallInputClass}
                        />
                        <button
                          onClick={addHeader}
                          disabled={!newHeaderKey.trim()}
                          className={addBtnClass}
                        >
                          Add
                        </button>
                      </div>
                    </section>

                    {/* OAuth Authentication */}
                    <section>
                      <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                        OAuth Authentication (Optional)
                      </label>
                      <div className="grid grid-cols-2 gap-4">
                        <div>
                          <label className="block text-[11px] text-[#C8CAD0] mb-1">Client ID</label>
                          <input
                            type="text"
                            value={config.oauth?.clientId || ""}
                            onChange={(e) => config && updateConfig({ oauth: { ...(config.oauth || {}), clientId: e.target.value } })}
                            placeholder="OAuth Client ID"
                            className={inputClass}
                          />
                        </div>
                        <div>
                          <label className="block text-[11px] text-[#C8CAD0] mb-1">Client Secret</label>
                          <input
                            type="password"
                            value={config.oauth?.clientSecret || ""}
                            onChange={(e) => config && updateConfig({ oauth: { ...(config.oauth || {}), clientSecret: e.target.value } })}
                            placeholder="OAuth Client Secret"
                            className={inputClass}
                          />
                        </div>
                        <div>
                          <label className="block text-[11px] text-[#C8CAD0] mb-1">Scope</label>
                          <input
                            type="text"
                            value={config.oauth?.scope || ""}
                            onChange={(e) => config && updateConfig({ oauth: { ...(config.oauth || {}), scope: e.target.value } })}
                            placeholder="e.g. read write"
                            className={inputClass}
                          />
                        </div>
                        <div>
                          <label className="block text-[11px] text-[#C8CAD0] mb-1">Callback Port</label>
                          <input
                            type="number"
                            value={config.oauth?.callbackPort || ""}
                            onChange={(e) => config && updateConfig({ oauth: { ...(config.oauth || {}), callbackPort: e.target.value ? parseInt(e.target.value) : undefined } })}
                            placeholder="e.g. 3000"
                            className={inputClass}
                          />
                        </div>
                      </div>
                    </section>
                  </>
                )}

                {/* ── Timeout (common) ────────────────────────────────────── */}
                <section>
                  <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                    Timeout (ms)
                  </label>
                  <input
                    type="number"
                    value={config.timeout ?? ""}
                    onChange={(e) =>
                      updateConfig({
                        timeout: e.target.value ? parseInt(e.target.value, 10) : undefined,
                      })
                    }
                    placeholder="Optional — e.g. 5000"
                    className="w-48 bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-2 text-[13px] text-[#F8F8FA] placeholder-[#C8CAD0]/40 outline-none font-mono transition-colors"
                  />
                </section>
              </div>
            </div>
          </div>
        ) : (
          /* Empty state */
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-16 h-16 mx-auto mb-6 rounded-2xl bg-[#F59E0B]/10 border border-[#F59E0B]/20 flex items-center justify-center">
              <Server size={24} className={ICONS.mcp.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-[#F8F8FA] mb-2">MCP Servers</h2>
            <p className="text-[14px] text-[#C8CAD0] mb-8 leading-relaxed max-w-sm">
              Configure Model Context Protocol servers that give your agents access to filesystems,
              databases, and developer tools. Add them here, then assign them to projects.
            </p>
            <div className="flex items-center gap-3">
              <button
                onClick={startCreate}
                className="px-4 py-2 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded shadow-sm transition-colors"
              >
                Add MCP Server
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
