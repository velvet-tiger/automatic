import { HardDrive, Globe } from "lucide-react";

/**
 * Flexible author descriptor for skills, MCP servers, and templates.
 *
 * Origins:
 *   - local  : created by the user locally
 *   - github : installed from a GitHub-hosted skill repo (skills.sh / SkillStore)
 *   - provider: installed from a named marketplace provider (MCP marketplace)
 */
export type AuthorInfo =
  | { type: "local" }
  | { type: "github"; repo: string; url?: string }        // e.g. "vercel-labs/agent-skills"
  | { type: "provider"; name: string; url?: string };     // e.g. { name: "Anthropic", url: "https://anthropic.com" }

interface AuthorBadgeProps {
  author: AuthorInfo;
  /** Display variant — defaults to "full" */
  variant?: "full" | "compact";
}

/**
 * A small, inline badge that shows the origin of an item.
 *
 * - local     → grey "local" label with HardDrive icon
 * - github    → green repo link with GitHub icon
 * - provider  → teal provider name, optionally linked
 */
export function AuthorBadge({ author, variant = "full" }: AuthorBadgeProps) {
  if (author.type === "local") {
    return (
      <span className="inline-flex items-center gap-1 text-[11px] text-text-muted">
        <HardDrive size={10} className="shrink-0" />
        {variant === "full" && <span>local</span>}
      </span>
    );
  }

  if (author.type === "github") {
    const href = author.url ?? `https://github.com/${author.repo}`;
    return (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        onClick={(e) => e.stopPropagation()}
        className="inline-flex items-center gap-1 text-[11px] text-success hover:text-success/80 transition-colors"
        title={author.repo}
      >
        {/* GitHub mark */}
        <svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor" className="shrink-0 opacity-80">
          <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z" />
        </svg>
        {variant === "full" && (
          <span className="truncate max-w-[180px]">{author.repo}</span>
        )}
      </a>
    );
  }

  // provider
  const label = author.name;
  const inner = (
    <span className="inline-flex items-center gap-1 text-[11px] text-[#6EC6C6] hover:text-[#8DD9D9] transition-colors">
      <Globe size={10} className="shrink-0 opacity-80" />
      {variant === "full" && <span className="truncate max-w-[160px]">{label}</span>}
    </span>
  );

  if (author.url) {
    return (
      <a
        href={author.url}
        target="_blank"
        rel="noopener noreferrer"
        onClick={(e) => e.stopPropagation()}
        title={label}
      >
        {inner}
      </a>
    );
  }

  return inner;
}

/**
 * A labelled "Author" row for use in detail panels.
 *
 * Usage:
 *   <AuthorRow author={{ type: "local" }} />
 *   <AuthorRow author={{ type: "github", repo: "vercel-labs/agent-skills" }} />
 *   <AuthorRow author={{ type: "provider", name: "Anthropic", url: "https://anthropic.com" }} />
 */
export function AuthorRow({ author }: { author: AuthorInfo }) {
  return (
    <div className="flex items-center gap-2">
      <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase shrink-0">
        Author
      </span>
      <AuthorBadge author={author} variant="full" />
    </div>
  );
}
