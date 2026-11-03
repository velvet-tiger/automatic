import { activityMeta, relativeTime } from "../helpers";
import type { ActivityEntry } from "../types";
import { SummaryCard } from "./SummaryCharts";
import { groupActivity, visibleActivityDetail, type ActivityGroup } from "./summaryStats";

const MAX_ROWS = 5;

interface ActivityFeedProps {
  entries: ActivityEntry[];
  loading: boolean;
  failed: boolean;
}

function repeatText(count: number): string {
  if (count <= 1) return "";
  return count === 2 ? "twice" : `${count} times`;
}

function ActivityRow({ group, last }: { group: ActivityGroup; last: boolean }) {
  const { entry, count } = group;
  const { icon } = activityMeta(entry.event);
  const detail = [repeatText(count), visibleActivityDetail(entry)].filter(Boolean).join(", ");
  return (
    <div className={`flex items-center gap-3 py-2 ${last ? "" : "border-b border-border-strong/10"}`}>
      <div className="flex-shrink-0 text-text-muted opacity-70">{icon}</div>
      <div className="min-w-0 flex-1 truncate" title={entry.label}>
        <span className="text-[12px] text-text-base">{entry.label}</span>
        {detail && <span className="ml-1.5 text-[12px] text-text-muted">{detail}</span>}
      </div>
      <span className="flex-shrink-0 text-[11px] text-text-muted">{relativeTime(entry.timestamp)}</span>
    </div>
  );
}

export function ActivityFeed({ entries, loading, failed }: ActivityFeedProps) {
  const groups = groupActivity(entries, MAX_ROWS);
  return (
    <SummaryCard title="Recent activity">
      {loading ? (
        <div className="py-1 text-[12px] text-text-muted">Loading activity…</div>
      ) : failed ? (
        <div className="py-1 text-[12px] text-text-muted">Could not load activity.</div>
      ) : groups.length === 0 ? (
        <div className="py-1 text-[12px] italic text-text-muted">
          No activity yet. Save or sync the project to start recording events.
        </div>
      ) : (
        <div className="-my-2">
          {groups.map((group, index) => (
            <ActivityRow key={group.entry.id} group={group} last={index === groups.length - 1} />
          ))}
        </div>
      )}
    </SummaryCard>
  );
}
