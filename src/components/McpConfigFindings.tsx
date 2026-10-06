import { AlertCircle, AlertTriangle, Info } from "lucide-react";
import type { McpConfigFinding, McpFindingSeverity } from "../lib/mcpConfigValidation";

const SEVERITY_STYLE: Record<McpFindingSeverity, { icon: typeof Info; className: string }> = {
  error: { icon: AlertCircle, className: "text-danger" },
  warning: { icon: AlertTriangle, className: "text-warning" },
  info: { icon: Info, className: "text-text-muted" },
};

/** Render a message, showing backtick-wrapped text as code. */
function renderMessage(message: string) {
  return message.split("`").map((part, i) =>
    i % 2 === 1 ? (
      <code key={i} className="font-mono">
        {part}
      </code>
    ) : (
      <span key={i}>{part}</span>
    ),
  );
}

/**
 * Validation findings for an MCP server config, most severe first as the
 * backend returns them. Renders nothing when there are no findings.
 */
export function McpConfigFindings({ findings }: { findings: McpConfigFinding[] }) {
  if (findings.length === 0) return null;
  return (
    <ul className="mt-2 space-y-1.5">
      {findings.map((finding) => {
        const { icon: Icon, className } = SEVERITY_STYLE[finding.severity];
        return (
          <li
            key={finding.code + finding.message}
            className={`flex items-start gap-1.5 text-[11px] leading-relaxed ${className}`}
          >
            <Icon size={12} className="mt-0.5 flex-shrink-0" />
            <span>{renderMessage(finding.message)}</span>
          </li>
        );
      })}
    </ul>
  );
}
