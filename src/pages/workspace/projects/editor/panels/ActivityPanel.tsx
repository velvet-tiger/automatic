// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { ChevronLeft, ChevronRight, History, RefreshCw } from "lucide-react";
import { activityMeta, relativeTime } from "../../helpers";
import type { ActivityEntry } from "../../types";

const ACTIVITY_PAGE_SIZE = 50;

interface ActivityPanelProps {
  projectName: string;
  activityPageEntries: ActivityEntry[];
  activityPage: number;
  activityTotalCount: number;
  loadingActivityPage: boolean;
  reloadActivityPage: (projectName: string, page: number) => void;
}

export function ActivityPanel({
  projectName,
  activityPageEntries,
  activityPage,
  activityTotalCount,
  loadingActivityPage,
  reloadActivityPage,
}: ActivityPanelProps) {
  const totalPages = Math.max(1, Math.ceil(activityTotalCount / ACTIVITY_PAGE_SIZE));
  return (
    <section className="flex flex-col gap-0">
      {/* Header row */}
      <div className="flex items-center justify-between px-1 pb-3 flex-shrink-0">
        <div className="flex items-center gap-2">
          <History size={13} className="text-text-muted" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Activity Log</span>
          {activityTotalCount > 0 && (
            <span className="text-[11px] text-text-muted">({activityTotalCount} total)</span>
          )}
        </div>
        <button
          onClick={() => reloadActivityPage(projectName, activityPage)}
          disabled={loadingActivityPage}
          className="text-[11px] text-text-muted hover:text-text-base transition-colors flex items-center gap-1 disabled:opacity-40"
        >
          <RefreshCw size={11} className={loadingActivityPage ? "animate-spin" : ""} />
          Refresh
        </button>
      </div>

      {/* Entries list */}
      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
        {loadingActivityPage ? (
          <div className="px-4 py-8 text-center text-[12px] text-text-muted">
            <RefreshCw size={14} className="animate-spin mx-auto mb-2" />
            Loading activity…
          </div>
        ) : activityPageEntries.length === 0 ? (
          <div className="px-4 py-8 text-center text-[12px] text-text-muted italic">
            No activity recorded yet. Save or sync the project to start logging events.
          </div>
        ) : (
          activityPageEntries.map((item, i) => {
            const { icon, dot } = activityMeta(item.event);
            return (
              <div
                key={item.id}
                className={`flex items-center gap-3 px-4 py-2.5 ${i < activityPageEntries.length - 1 ? "border-b border-border-strong/20" : ""}`}
              >
                <div className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${dot}`} />
                <div className="flex-shrink-0 text-text-muted">{icon}</div>
                <div className="flex-1 min-w-0">
                  <span className="text-[12px] text-text-base">{item.label}</span>
                  {item.detail && (
                    <span className="text-[12px] text-text-muted ml-1.5">{item.detail}</span>
                  )}
                </div>
                <span className="text-[11px] text-text-muted flex-shrink-0">{relativeTime(item.timestamp)}</span>
              </div>
            );
          })
        )}
      </div>

      {/* Pagination controls */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between pt-3 flex-shrink-0">
          <button
            onClick={() => reloadActivityPage(projectName, activityPage - 1)}
            disabled={activityPage === 0 || loadingActivityPage}
            className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium text-text-muted hover:text-text-base border border-border-strong/40 rounded-md disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
          >
            <ChevronLeft size={13} /> Previous
          </button>
          <span className="text-[12px] text-text-muted">
            Page {activityPage + 1} of {totalPages}
          </span>
          <button
            onClick={() => reloadActivityPage(projectName, activityPage + 1)}
            disabled={activityPage >= totalPages - 1 || loadingActivityPage}
            className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium text-text-muted hover:text-text-base border border-border-strong/40 rounded-md disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
          >
            Next <ChevronRight size={13} />
          </button>
        </div>
      )}
    </section>
  );
}
