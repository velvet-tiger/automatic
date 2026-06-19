// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import mcpServersData from "../../../../../src-tauri/assets/discover/featured-mcp-servers.json";
import { AlertCircle, Plus, RefreshCw } from "lucide-react";
import type { ProjectRecommendation } from "../types";

/**
 * Installs an AI-suggested MCP server config and adds it to the project.
 *
 * Looks up the server in the featured Discover data by title/slug match,
 * builds its config JSON, saves it via `save_mcp_server_config`, then notifies
 * the parent to add it to the project config.
 *
 * On failure the button shows an error — nothing is added to the project.
 */
export function McpAddButton({
  rec,
  alreadyAdded,
  onAdd,
}: {
  rec: ProjectRecommendation;
  alreadyAdded: boolean;
  onAdd: (serverName: string) => Promise<boolean> | boolean;
}) {
  const [state, setState] = useState<"idle" | "loading" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const handleAdd = async () => {
    setState("loading");
    setErrorMsg(null);

    // Find the server in the local Discover catalogue by slug or title.
    const needle = rec.title.toLowerCase();
    const servers = mcpServersData as Array<{
      slug: string; name: string; title: string; provider: string;
      repository_url: string | null;
      remote: { transport: string; url: string } | null;
      local: { registry: string; package: string; version: string | null; transport: string; command: string; args?: string[] | null } | null;
      auth: { method: string; env_vars: Array<{ name: string; description: string; secret: boolean }> };
    }>;
    const server = servers.find(
      (s) => s.slug === needle || s.title.toLowerCase() === needle || s.name.toLowerCase() === needle,
    );

    if (!server) {
      setState("error");
      setErrorMsg("Server not found in the Discover catalogue. Use Discover MCP Servers to add it manually.");
      return;
    }

    // Build the config — same logic as DiscoverMcp.buildConfig.
    const _author: Record<string, string> = { name: server.provider };
    if (server.repository_url) _author.repository_url = server.repository_url;

    let config: Record<string, unknown>;
    if (server.local) {
      // Mirrors resolveLocalCommand in DiscoverMcp: prefer an explicit args
      // vector, fall back to splitting the command string for legacy entries.
      const parts = server.local.command.split(/\s+/).filter(Boolean);
      const cmd = parts[0] ?? "";
      const args = server.local.args ?? parts.slice(1);
      const env: Record<string, string> = {};
      server.auth.env_vars.forEach((v) => { env[v.name] = ""; });
      config = { type: "stdio", command: cmd, _author };
      if (args.length > 0) config.args = args;
      if (Object.keys(env).length > 0) config.env = env;
    } else if (server.remote) {
      const type = server.remote.transport === "sse" ? "sse" : "http";
      config = { type, url: server.remote.url, _author };
    } else {
      config = { type: "stdio", command: "", _author };
    }

    // Derive the config key — same as DiscoverMcp.configName.
    const configKey = server.title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

    try {
      await invoke("save_mcp_server_config", { name: configKey, data: JSON.stringify(config) });
    } catch (err: any) {
      setState("error");
      setErrorMsg(`Failed to save server config: ${err}`);
      return;
    }

    setState("idle");
    const added = await Promise.resolve(onAdd(configKey));
    if (!added) {
      setState("error");
      setErrorMsg("Server was saved, but failed to add it to this project.");
      return;
    }
  };

  if (errorMsg) {
    return (
      <span className="text-[11px] text-error flex items-center gap-1">
        <AlertCircle size={10} /> {errorMsg}
      </span>
    );
  }

  return (
    <button
      onClick={handleAdd}
      disabled={alreadyAdded || state === "loading"}
      className="text-[11px] font-medium text-brand hover:text-brand-hover border border-brand/40 rounded px-2 py-1 transition-colors flex items-center gap-1 disabled:opacity-40 disabled:cursor-default"
    >
      {state === "loading"
        ? <><RefreshCw size={10} className="animate-spin" /> Adding…</>
        : <><Plus size={10} /> {alreadyAdded ? "Added" : "Add to project"}</>
      }
    </button>
  );
}
