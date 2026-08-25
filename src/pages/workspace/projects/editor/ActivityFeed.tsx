// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { History } from "lucide-react";
import { activityMeta, relativeTime } from "../helpers";
import type { ActivityEntry } from "../types";

interface ActivityFeedProps {
  entries: ActivityEntry[];
  loading: boolean;
}

export function ActivityFeed({ entries, loading }: ActivityFeedProps) {
  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <History size={13} className="text-text-muted" />
        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Recent Activity</span>
      </div>
      <div className="rounded-lg bg-bg-input/60 px-3">
        {loading ? (
          <div className="py-3 text-[12px] text-text-muted">Loading activity…</div>
        ) : entries.length === 0 ? (
          <div className="py-3 text-[12px] text-text-muted italic">
            No activity yet. Save or sync the project to start recording events.
          </div>
        ) : (
          entries.map((item, i) => {
            const { icon } = activityMeta(item.event);
            return (
              <div
                key={item.id}
                className={`flex items-center gap-3 py-2 ${i < entries.length - 1 ? "border-b border-border-strong/10" : ""}`}
              >
                <div className="flex-shrink-0 text-text-muted opacity-70">{icon}</div>
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
    </section>
  );
}
