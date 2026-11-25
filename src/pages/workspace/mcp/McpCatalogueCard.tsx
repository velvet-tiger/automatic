import { BookOpen, Compass, ExternalLink } from "lucide-react";
import { McpServerIcon } from "../../../components/McpServerIcon";
import { handleExternalLinkClick } from "../../../lib/externalLinks";
import type { DiscoverCatalogueEntry } from "../../../lib/discoverMcpMatch";

const CLASSIFICATION_COLORS: Record<string, string> = {
  official: "bg-brand/15 text-brand border-brand/20",
  reference: "bg-accent/15 text-accent border-accent/20",
  community: "bg-accent-hover/15 text-accent-hover border-accent-hover/20",
};

interface McpCatalogueCardProps {
  entry: DiscoverCatalogueEntry;
  /** Opens this entry in Discover. The link is hidden when not given. */
  onViewInDiscover?: (slug: string) => void;
}

/**
 * The Discover catalogue details of a server installed from Discover: logo,
 * title, provider, description, and links to Discover and the docs. Shown in
 * place of the plain author panel.
 */
export function McpCatalogueCard({ entry, onViewInDiscover }: McpCatalogueCardProps) {
  const badge = CLASSIFICATION_COLORS[entry.classification] ?? "bg-surface text-text-muted border-border-strong/40";
  return (
    <div className="flex items-start gap-4">
      <McpServerIcon icon={entry.icon} title={entry.title} size={44} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-[14px] font-medium text-text-base">{entry.title}</span>
          <span className={`inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium border ${badge}`}>
            {entry.classification}
          </span>
        </div>
        <p className="text-[12px] text-text-muted">By {entry.provider}</p>
        <p className="mt-2 text-[12px] text-text-muted leading-relaxed">{entry.description}</p>
        <div className="mt-2 flex items-center gap-4">
          {onViewInDiscover && (
            <button
              onClick={() => onViewInDiscover(entry.slug)}
              className="inline-flex items-center gap-1 text-[11px] text-brand hover:text-brand-hover"
            >
              <Compass size={11} /> View in Discover
            </button>
          )}
          {entry.docs_url && (
            <a
              href={entry.docs_url}
              target="_blank"
              rel="noopener noreferrer"
              onClick={handleExternalLinkClick(entry.docs_url)}
              className="inline-flex items-center gap-1 text-[11px] text-brand hover:text-brand-hover"
            >
              <BookOpen size={11} /> Documentation <ExternalLink size={10} />
            </a>
          )}
        </div>
      </div>
    </div>
  );
}
