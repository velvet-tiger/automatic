import { useEffect, useState, type ReactNode } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import type { ProjectNotice, ProjectNoticeSeverity } from "../../types";
import {
  NOTICE_SEVERITIES,
  countNotices,
  problemIds,
  readHiddenNotices,
  staysHidden,
  writeHiddenNotices,
} from "../../notices";

const COUNT_STYLES: Record<ProjectNoticeSeverity, string> = {
  problem: "text-danger border-danger/50",
  warning: "text-warning border-warning/50",
  notice: "text-text-muted border-border-strong",
};

const COUNT_WORDS: Record<ProjectNoticeSeverity, [string, string]> = {
  problem: ["problem", "problems"],
  warning: ["warning", "warnings"],
  notice: ["notice", "notices"],
};

export function countLabel(severity: ProjectNoticeSeverity, count: number): string {
  const [one, many] = COUNT_WORDS[severity];
  return `${count} ${count === 1 ? one : many}`;
}

interface ProjectNoticeListProps {
  /** local_key of the open project. The hidden state is kept per project. */
  projectKey: string;
  /** Already sorted, most urgent first. */
  notices: ProjectNotice[];
  renderNotice: (notice: ProjectNotice) => ReactNode;
}

/**
 * The one list of problems, warnings and notices at the top of the project
 * editor. The user can hide it; the counts stay in view, and a new problem
 * opens it again. Takes no space when there is nothing to show.
 */
export function ProjectNoticeList({ projectKey, notices, renderNotice }: ProjectNoticeListProps) {
  // The problem ids the user saw when they hid the list, or null when open.
  const [hiddenWith, setHiddenWith] = useState<string[] | null>(() => readHiddenNotices(projectKey));

  useEffect(() => {
    setHiddenWith(readHiddenNotices(projectKey));
  }, [projectKey]);

  const reopened = hiddenWith !== null && !staysHidden(hiddenWith, notices);
  useEffect(() => {
    if (!reopened) return;
    writeHiddenNotices(projectKey, null);
    setHiddenWith(null);
  }, [reopened, projectKey]);

  if (notices.length === 0) return null;

  const open = hiddenWith === null || reopened;
  const counts = countNotices(notices);
  const heading = counts.problem + counts.warning > 0 ? "Needs attention" : "Notices";
  const listId = `project-notices-${projectKey}`;

  const toggle = () => {
    const next = open ? problemIds(notices) : null;
    writeHiddenNotices(projectKey, next);
    setHiddenWith(next);
  };

  return (
    <section aria-label={heading} className="flex-shrink-0 border-b border-border-strong/40">
      <button
        type="button"
        onClick={toggle}
        aria-expanded={open}
        aria-controls={listId}
        className="flex w-full flex-wrap items-center gap-2.5 px-6 py-2 text-left hover:bg-bg-input/40 transition-colors"
      >
        <span className="text-[12px] font-semibold text-text-base">{heading}</span>
        <span className="flex flex-wrap gap-1.5">
          {NOTICE_SEVERITIES.filter((s) => counts[s] > 0).map((s) => (
            <span key={s} className={`rounded-full border px-2 py-px text-[11px] font-medium tabular-nums ${COUNT_STYLES[s]}`}>
              {countLabel(s, counts[s])}
            </span>
          ))}
        </span>
        <span className="ml-auto flex items-center gap-1 text-[11px] text-text-muted">
          {open ? "Hide" : "Show"}
          {open ? <ChevronUp size={12} aria-hidden="true" /> : <ChevronDown size={12} aria-hidden="true" />}
        </span>
      </button>
      {open && (
        <div id={listId} className="max-h-[40vh] overflow-y-auto">
          {notices.map((notice) => (
            <div key={notice.id}>{renderNotice(notice)}</div>
          ))}
        </div>
      )}
    </section>
  );
}
