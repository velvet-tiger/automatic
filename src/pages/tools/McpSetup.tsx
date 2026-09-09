import { useCallback, useState } from "react";
import { Check, Copy, Puzzle } from "lucide-react";

// ── Config snippets ───────────────────────────────────────────────────────────

interface ConfigSnippet {
  title: string;
  file: string;
  note: string;
  code: string;
}

const SNIPPETS: ConfigSnippet[] = [
  {
    title: "Claude Code",
    file: ".mcp.json",
    note: "Project root. Already written automatically for projects registered in Automatic.",
    code: `{
  "mcpServers": {
    "automatic": {
      "type": "stdio",
      "command": "automatic",
      "args": ["mcp-serve"]
    }
  }
}`,
  },
  {
    title: "Claude Desktop",
    file: "claude_desktop_config.json",
    note: "On macOS: ~/Library/Application Support/Claude/claude_desktop_config.json",
    code: `{
  "mcpServers": {
    "automatic": {
      "command": "automatic",
      "args": ["mcp-serve"]
    }
  }
}`,
  },
  {
    title: "Cursor",
    file: "~/.cursor/mcp.json",
    note: "Global config. Use .cursor/mcp.json in a project root for project scope.",
    code: `{
  "mcpServers": {
    "automatic": {
      "type": "stdio",
      "command": "automatic",
      "args": ["mcp-serve"]
    }
  }
}`,
  },
  {
    title: "Codex CLI",
    file: "~/.codex/config.toml",
    note: "Global config, shared by every project.",
    code: `[mcp_servers.automatic]
command = "automatic"
args = ["mcp-serve"]`,
  },
  {
    title: "OpenCode",
    file: "opencode.json",
    note: "Project root, or ~/.opencode.json for global scope.",
    code: `{
  "mcp": {
    "automatic": {
      "type": "local",
      "command": ["automatic", "mcp-serve"]
    }
  }
}`,
  },
];

// ── Components ────────────────────────────────────────────────────────────────

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      className="flex-shrink-0 p-1.5 rounded hover:bg-surface transition-colors"
      title="Copy to clipboard"
    >
      {copied ? (
        <Check size={12} className="text-success" />
      ) : (
        <Copy size={12} className="text-text-muted" />
      )}
    </button>
  );
}

function ConfigCard({ snippet }: { snippet: ConfigSnippet }) {
  return (
    <div className="bg-bg-input border border-border-strong/40 rounded-xl overflow-hidden">
      <div className="flex items-center gap-3 px-4 py-3 border-b border-border-strong/30">
        <div className="flex-1 min-w-0">
          <h3 className="text-[13px] font-semibold text-text-base">{snippet.title}</h3>
          <p className="text-[11px] text-text-muted font-mono truncate">{snippet.file}</p>
        </div>
        <CopyButton text={snippet.code} />
      </div>
      <pre className="px-4 py-3 bg-bg-sidebar text-[12px] font-mono text-text-base leading-relaxed overflow-x-auto">
        <code>{snippet.code}</code>
      </pre>
      <p className="px-4 py-2.5 text-[11px] text-text-muted border-t border-border-strong/30">
        {snippet.note}
      </p>
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function McpSetup() {
  return (
    <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-bg-base">
      <div className="max-w-3xl mx-auto space-y-8">
        {/* Header */}
        <div className="flex items-start gap-4">
          <div className="p-3 rounded-xl bg-icon-mcp/10 border border-icon-mcp/20 shrink-0">
            <Puzzle size={20} className="text-icon-mcp" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold text-text-base mb-2">MCP Setup</h1>
            <p className="text-text-muted text-[13px] leading-relaxed max-w-2xl">
              Add Automatic's MCP server to any MCP-compatible tool. Projects registered
              in Automatic get this configured on every sync. Use these snippets for tools
              Automatic does not manage.
            </p>
          </div>
        </div>

        {/* How it runs */}
        <div className="bg-bg-input border border-border-strong/40 rounded-xl p-4">
          <h2 className="text-[13px] font-semibold text-text-base mb-2">The command</h2>
          <p className="text-[12px] text-text-muted leading-relaxed mb-3">
            The server runs over stdio from the same binary as the app. The{" "}
            <code className="text-text-base">automatic</code> CLI is installed at{" "}
            <code className="text-text-base">/usr/local/bin/automatic</code> on macOS and
            Linux. If a tool cannot find <code className="text-text-base">automatic</code>{" "}
            on its PATH, use the full path as the command.
          </p>
          <pre className="px-3 py-2 rounded-md bg-bg-sidebar text-[12px] font-mono text-text-base overflow-x-auto">
            <code>automatic mcp-serve</code>
          </pre>
        </div>

        {/* Per-tool configs */}
        <div className="space-y-4">
          <h2 className="text-[13px] font-semibold text-text-base">Tool configurations</h2>
          {SNIPPETS.map((snippet) => (
            <ConfigCard key={snippet.title} snippet={snippet} />
          ))}
        </div>
      </div>
    </div>
  );
}
