/** One clickable segment in the muted inventory line on Summary. */
export interface SummaryInventoryItem {
  label: string;
  count: number;
  onView: () => void;
}

interface SummaryInventoryRowProps {
  items: SummaryInventoryItem[];
}

/**
 * Quiet metrics strip — same visual language as overview card hover metrics.
 * Each segment opens its related project tab.
 */
export function SummaryInventoryRow({ items }: SummaryInventoryRowProps) {
  return (
    <div className="flex flex-wrap items-center gap-x-1 gap-y-1 text-[12px] text-text-muted/50">
      {items.map((item, index) => (
        <span key={item.label} className="inline-flex items-center gap-x-1">
          {index > 0 && <span aria-hidden className="text-text-muted/30">·</span>}
          <button
            type="button"
            onClick={item.onView}
            className="tabular-nums transition-colors hover:text-text-base"
          >
            {item.count} {item.label}
          </button>
        </span>
      ))}
    </div>
  );
}

export function SummarySidebarSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border border-border-strong/35 bg-bg-input px-3 py-2.5">
      <div className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-text-muted/60">
        {title}
      </div>
      <div className="space-y-2">{children}</div>
    </section>
  );
}
