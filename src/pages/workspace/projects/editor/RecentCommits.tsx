import { relativeTime } from "../helpers";
import type { GitCommit } from "../types";
import { SummaryCard } from "./SummaryCharts";

const MAX_ROWS = 5;

function CommitRow({ commit, last }: { commit: GitCommit; last: boolean }) {
  const lines = commit.insertions + commit.deletions > 0
    ? `+${commit.insertions} −${commit.deletions}`
    : "";
  return (
    <div className={`flex items-center gap-3 py-2 ${last ? "" : "border-b border-border-strong/10"}`}>
      <div className="min-w-0 flex-1 truncate" title={`${commit.subject}\n${commit.author} · ${commit.hash.slice(0, 8)}`}>
        <span className="text-[12px] text-text-base">{commit.subject}</span>
        {commit.touches_agent_config && (
          <span className="ml-1.5 rounded bg-brand/10 px-1.5 py-px text-[10px] text-brand">Agent config</span>
        )}
        <span className="ml-1.5 text-[12px] text-text-muted">{commit.author}</span>
      </div>
      {lines && <span className="flex-shrink-0 text-[11px] tabular-nums text-text-muted">{lines}</span>}
      <span className="flex-shrink-0 text-[11px] text-text-muted">{relativeTime(commit.timestamp)}</span>
    </div>
  );
}

/** The newest commits that touched the project folder. */
export function RecentCommits({
  commits,
  historyDays,
  loading,
}: {
  commits: GitCommit[];
  historyDays: number;
  loading: boolean;
}) {
  const rows = commits.slice(0, MAX_ROWS);
  return (
    <SummaryCard title="Recent commits">
      {loading ? (
        <div className="py-1 text-[12px] text-text-muted">Loading commits…</div>
      ) : rows.length === 0 ? (
        <div className="py-1 text-[12px] text-text-muted">
          No commits in the last {Math.round(historyDays / 7)} weeks.
        </div>
      ) : (
        <div className="-my-2">
          {rows.map((commit, index) => (
            <CommitRow key={commit.hash} commit={commit} last={index === rows.length - 1} />
          ))}
        </div>
      )}
    </SummaryCard>
  );
}
