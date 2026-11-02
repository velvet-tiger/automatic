import { AlertTriangle } from "lucide-react";
import {
  libraryFlagReason,
  libraryKindLabel,
  type LibraryUseGroup,
} from "../lib/libraryUse";

interface LibraryUseListProps {
  groups: LibraryUseGroup[];
}

/**
 * The library items a project's config would use. Names come from an
 * untrusted repository, so they render as plain text and wrap. Flagged items
 * stay visible, and the rest collapse by kind.
 */
export function LibraryUseList({ groups }: LibraryUseListProps) {
  const flagged = groups.flatMap((g) =>
    g.names
      .filter((n) => n.flagged)
      .map((n) => ({ ...n, kind: g.kind })),
  );
  const unchecked = groups.reduce((sum, g) => sum + g.not_checked, 0);

  return (
    <div className="rounded-lg border border-border-strong/40 bg-bg-input px-3 py-2 space-y-2">
      <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted">
        Used from your library
      </div>

      {groups.length === 0 && (
        <p className="text-[12px] text-text-muted">
          This configuration does not use anything from your library.
        </p>
      )}

      {flagged.length > 0 && (
        <ul className="space-y-1">
          {flagged.map((item) => (
            <li key={`${item.kind}:${item.name}`} className="flex items-start gap-2 text-[12px]">
              <AlertTriangle size={12} className="text-warning mt-0.5 shrink-0" />
              <span className="font-mono text-text-base break-all">{item.name}</span>
              <span className="text-text-muted shrink-0">
                {libraryKindLabel(item.kind)}: {libraryFlagReason(item.kind)}
              </span>
            </li>
          ))}
        </ul>
      )}

      {groups.map((group) => {
        const hidden = group.total - group.names.length;
        return (
          <details key={group.kind} className="text-[12px]">
            <summary className="cursor-pointer text-text-base">
              {libraryKindLabel(group.kind)} ({group.total})
            </summary>
            <ul className="mt-1 ml-3 max-h-32 overflow-y-auto space-y-0.5">
              {group.names.map((n) => (
                <li key={n.name} className="font-mono text-text-muted break-all">
                  {n.name}
                </li>
              ))}
              {hidden > 0 && <li className="text-text-muted">and {hidden} more</li>}
            </ul>
          </details>
        );
      })}

      {unchecked > 0 && (
        <p className="text-[12px] text-warning">
          This configuration lists too many names to check. {unchecked} were not checked, so
          read-only mode is the safer choice.
        </p>
      )}
    </div>
  );
}
