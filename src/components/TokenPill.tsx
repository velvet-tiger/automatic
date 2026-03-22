/**
 * TokenPill — a small inline token-count indicator.
 *
 * Renders a subtle pill showing the approximate Claude token count for the
 * supplied text. Intended to be placed near text areas and content panels
 * throughout the application.
 *
 * Counting is done entirely client-side using a character-ratio heuristic
 * (3.8 chars / token) so there is no backend round-trip and it updates
 * instantly as the user types. The result is labelled "~" to make clear it
 * is an approximation.
 */

import { useMemo } from "react";

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Estimate tokens using Claude's ~3.8 chars/token ratio.
 * Returns 0 for empty/whitespace-only input.
 */
function estimateTokens(text: string): number {
  if (!text || text.trim().length === 0) return 0;
  return Math.ceil(text.length / 3.8);
}

/**
 * Format a token count into a compact human-readable string.
 * e.g. 1234 → "1.2K", 1500000 → "1.5M"
 */
function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

// ── Component ─────────────────────────────────────────────────────────────────

interface TokenPillProps {
  /** The text content to estimate tokens for. */
  text: string;
  /** Optional CSS class overrides. */
  className?: string;
}

/**
 * A small pill that shows the approximate token count for the given text.
 * Renders nothing when the text is empty.
 */
export function TokenPill({ text, className }: TokenPillProps) {
  const tokens = useMemo(() => estimateTokens(text), [text]);

  if (tokens === 0) return null;

  return (
    <span
      className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-mono text-text-muted bg-bg-sidebar border border-border-strong/25 select-none tabular-nums ${className ?? ""}`}
      title={`~${tokens.toLocaleString()} tokens (Claude approximation, 3.8 chars/token)`}
    >
      ~{formatTokens(tokens)}
      <span className="text-[9px] opacity-60">tok</span>
    </span>
  );
}
