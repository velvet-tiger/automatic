// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

interface SummaryMetricCardProps {
  icon: React.ReactNode;
  label: string;
  count: number;
  accentClass: string;
  onView: () => void;
}

export function SummaryMetricCard({ icon, label, count, accentClass, onView }: SummaryMetricCardProps) {
  return (
    <section
      role="button"
      tabIndex={0}
      onClick={onView}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onView(); } }}
      className="cursor-pointer rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3 transition-colors hover:border-border-strong hover:bg-bg-input/80"
    >
      <div className="flex items-center gap-2">
        <div className={`shrink-0 rounded-md p-1.5 ${accentClass}`}>{icon}</div>
        <span className="truncate text-[13px] font-semibold text-text-base">{label}</span>
        <span className="ml-auto text-[18px] font-semibold leading-none tabular-nums text-text-base">{count}</span>
      </div>
    </section>
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
    <section className="rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3">
      <div className="mb-3 text-[13px] font-semibold text-text-base">{title}</div>
      <div className="space-y-2">{children}</div>
    </section>
  );
}
