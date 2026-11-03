// Small SVG and div charts for the project Summary panel.

import { useState, type ReactNode } from "react";
import { AlertCircle, Check, Minus, Plus, RefreshCw, TriangleAlert } from "lucide-react";
import { activityLevel, type AgentSyncStatus, type CalendarDay } from "./summaryStats";

const LEVEL_CLASSES = [
  "bg-text-muted/10",
  "bg-brand/45",
  "bg-brand/65",
  "bg-brand/85",
  "bg-brand",
] as const;

export function SummaryCard({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: string;
  children: ReactNode;
}) {
  return (
    <section className="min-w-0 rounded-lg border border-border-strong/35 bg-bg-input px-4 py-3">
      <div className="mb-2.5 flex items-baseline justify-between gap-2">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-text-muted/60">
          {title}
        </span>
        {aside && <span className="text-[11px] text-text-muted">{aside}</span>}
      </div>
      {children}
    </section>
  );
}

const RING_RADIUS = 21;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

const RING_STYLES: Record<AgentSyncStatus, { stroke: string; icon: ReactNode }> = {
  synced: { stroke: "stroke-health-synced", icon: <Check size={18} className="text-success" /> },
  drifted: {
    stroke: "stroke-health-drifted",
    icon: <TriangleAlert size={16} className="text-warning" />,
  },
  checking: {
    stroke: "stroke-text-muted",
    icon: <RefreshCw size={14} className="text-text-muted" />,
  },
  failed: { stroke: "stroke-danger", icon: <AlertCircle size={16} className="text-danger" /> },
  not_set_up: { stroke: "stroke-text-muted", icon: <Minus size={16} className="text-text-muted" /> },
};

/** A ring filled by the share of agents in sync, with the state at its centre. */
export function SyncRing({ status, share }: { status: AgentSyncStatus; share: number }) {
  const { stroke, icon } = RING_STYLES[status];
  return (
    <div className="relative h-[52px] w-[52px] flex-shrink-0" aria-hidden>
      <svg width="52" height="52" viewBox="0 0 52 52" className="-rotate-90">
        <circle
          cx="26"
          cy="26"
          r={RING_RADIUS}
          fill="none"
          strokeWidth="6"
          className="stroke-border-strong/40"
        />
        <circle
          cx="26"
          cy="26"
          r={RING_RADIUS}
          fill="none"
          strokeWidth="6"
          strokeDasharray={`${share * RING_CIRCUMFERENCE} ${RING_CIRCUMFERENCE}`}
          className={stroke}
        />
      </svg>
      <div className="absolute inset-0 flex items-center justify-center">{icon}</div>
    </div>
  );
}

/** A trend line over `values`, stretched to the width of its container. */
export function Sparkline({ values }: { values: number[] }) {
  const width = 160;
  const height = 30;
  const max = Math.max(...values, 1);
  const points = values.map((value, index) => {
    const x = (index / Math.max(values.length - 1, 1)) * width;
    const y = height - 2 - (value / max) * (height - 4);
    return `${x.toFixed(1)} ${y.toFixed(1)}`;
  });
  const line = `M${points.join(" L")}`;
  return (
    <svg
      width="100%"
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      aria-hidden
      className="mt-1.5 block text-brand"
    >
      <path d={`${line} L${width} ${height} L0 ${height}Z`} fill="currentColor" fillOpacity={0.1} />
      <path
        d={line}
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

function dayText(day: CalendarDay): string {
  const date = day.date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
  return `${date}: ${day.count} ${day.count === 1 ? "event" : "events"}`;
}

/** Gives the calendar's content in words, for anyone who cannot use the grid. */
export function calendarSummary(days: CalendarDay[]): string {
  const total = days.reduce((sum, day) => sum + day.count, 0);
  if (total === 0) return "No events in this period.";
  const busiest = days.reduce((best, day) => (day.count > best.count ? day : best));
  const date = busiest.date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
  const events = `${total} ${total === 1 ? "event" : "events"}`;
  return `${events}. Busiest day ${date}, with ${busiest.count}.`;
}

export function ActivityCalendarGrid({
  days,
  rangeLabel,
}: {
  days: CalendarDay[];
  rangeLabel: string;
}) {
  const [hovered, setHovered] = useState<CalendarDay | null>(null);
  const summary = calendarSummary(days);
  return (
    <div onMouseLeave={() => setHovered(null)}>
      <div
        role="img"
        aria-label={`Activity calendar. ${rangeLabel}. ${summary}`}
        className="grid grid-flow-col grid-rows-7 justify-start gap-[3px]"
      >
        {days.map((day) => (
          <div
            key={day.key}
            onMouseEnter={() => setHovered(day.known ? day : null)}
            className={`h-[13px] w-[13px] rounded-[3px] ${
              day.known
                ? LEVEL_CLASSES[activityLevel(day.count)]
                : "border border-dashed border-text-muted/50"
            }`}
          />
        ))}
      </div>
      <p className="mt-2 min-h-[16px] text-[11px] text-text-muted">
        {hovered === null ? summary : dayText(hovered)}
      </p>
    </div>
  );
}

export interface ConfigItem {
  key: string;
  /** Name with its count, such as "18 skills". */
  countLabel: string;
  /** Name shown when there are none, such as "Hooks". */
  emptyLabel: string;
  count: number;
  colorClass: string;
  onView: () => void;
}

/** Share of each configuration type. Items with no entries become add links. */
export function ConfigBreakdown({ items }: { items: ConfigItem[] }) {
  const used = items.filter((item) => item.count > 0);
  return (
    <>
      {used.length > 0 ? (
        <div className="mb-3 flex gap-0.5">
          {used.map((item) => (
            <button
              key={item.key}
              type="button"
              onClick={item.onView}
              aria-label={`View ${item.countLabel}`}
              title={item.countLabel}
              style={{ flexGrow: item.count }}
              className={`h-3.5 min-w-1 basis-0 transition-opacity first:rounded-l last:rounded-r hover:opacity-80 ${item.colorClass}`}
            />
          ))}
        </div>
      ) : (
        <p className="mb-3 text-[12px] text-text-muted">Nothing added yet.</p>
      )}
      <div className="grid grid-cols-2 gap-x-4 gap-y-1.5">
        {items.map((item) => (
          <button
            key={item.key}
            type="button"
            onClick={item.onView}
            aria-label={item.count > 0 ? `View ${item.countLabel}` : `Add ${item.emptyLabel}`}
            className={`flex min-w-0 items-center gap-1.5 text-left text-[12px] tabular-nums transition-colors hover:text-text-base ${
              item.count > 0 ? "text-text-base" : "text-text-muted"
            }`}
          >
            {item.count > 0 ? (
              <span className={`h-2.5 w-2.5 flex-shrink-0 rounded-sm ${item.colorClass}`} />
            ) : (
              <Plus size={10} className="flex-shrink-0" />
            )}
            <span className="truncate">{item.count > 0 ? item.countLabel : item.emptyLabel}</span>
          </button>
        ))}
      </div>
    </>
  );
}
