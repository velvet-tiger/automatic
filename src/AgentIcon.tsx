import { Bot } from "lucide-react";

/**
 * Maps agent IDs to their SVG logo filenames in /agents/.
 *
 * Most IDs match 1:1 with the filename. Exceptions:
 *   - "claude" -> "claude-code.svg"
 *   - "kiro"   -> "kiro-cli.svg"
 *
 * Agents without an SVG (junie, warp) fall back to the generic Bot icon.
 */
const AGENT_LOGO_MAP: Record<string, string> = {
  claude: "claude-code",
  cursor: "cursor",
  copilot: "copilot",
  kilo: "kilo",
  cline: "cline",
  kiro: "kiro-cli",
  gemini: "gemini",
  antigravity: "antigravity",
  droid: "droid",
  goose: "goose",
  codex: "codex",
  opencode: "opencode",
};

interface AgentIconProps {
  /** The agent's string ID (e.g. "claude", "cursor") */
  agentId: string;
  /** Icon size in pixels (used for both width/height) */
  size: number;
  /** Optional extra className applied to the fallback Bot icon */
  className?: string;
}

/**
 * Renders the agent's SVG logo as a white icon (via CSS filter), or falls
 * back to the generic Bot lucide icon for agents without a logo.
 */
export function AgentIcon({ agentId, size, className }: AgentIconProps) {
  const logoFile = AGENT_LOGO_MAP[agentId];

  if (logoFile) {
    return (
      <img
        src={`/agents/${logoFile}.svg`}
        alt={agentId}
        width={size}
        height={size}
        className="brightness-0 invert flex-shrink-0"
        draggable={false}
      />
    );
  }

  return <Bot size={size} className={className ?? "text-text-base"} />;
}
