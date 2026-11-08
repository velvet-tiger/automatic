import type { ReactNode } from "react";
import { AlertCircle, AlertTriangle, Info } from "lucide-react";
import type { ProjectNoticeSeverity } from "../../types";

/**
 * Text colour for each severity. The raw --warning and --danger are too pale
 * on their own tint in the light themes, so the text mixes them with the
 * theme's text colour: darker in light themes, lighter in dark ones.
 */
export const SEVERITY_TEXT: Record<ProjectNoticeSeverity, string> = {
  problem: "text-[color-mix(in_srgb,var(--danger)_70%,var(--text-base))]",
  warning: "text-[color-mix(in_srgb,var(--warning)_60%,var(--text-base))]",
  notice: "text-text-base",
};

/**
 * Severity shows as an icon and a coloured left edge as well as a tint, so
 * the list still reads in themes that draw warnings and problems in grey.
 */
const ROW_STYLES: Record<ProjectNoticeSeverity, { row: string; title: string; icon: ReactNode }> = {
  problem: {
    row: "bg-danger/10 border-l-danger",
    title: SEVERITY_TEXT.problem,
    icon: <AlertCircle size={13} className="shrink-0 mt-0.5 text-danger" aria-hidden="true" />,
  },
  warning: {
    row: "bg-warning/10 border-l-warning",
    title: SEVERITY_TEXT.warning,
    icon: <AlertTriangle size={13} className="shrink-0 mt-0.5 text-warning" aria-hidden="true" />,
  },
  notice: {
    row: "bg-bg-input border-l-border-strong",
    title: SEVERITY_TEXT.notice,
    icon: <Info size={13} className="shrink-0 mt-0.5 text-text-muted" aria-hidden="true" />,
  },
};

const SEVERITY_LABELS: Record<ProjectNoticeSeverity, string> = {
  problem: "Problem",
  warning: "Warning",
  notice: "Notice",
};

interface NoticeRowProps {
  severity: ProjectNoticeSeverity;
  title: ReactNode;
  /** Detail under the title: text, fix steps, file chips. */
  children?: ReactNode;
  /** Buttons and links, shown on the right. */
  actions?: ReactNode;
  /** Set for a check that could not run, so screen readers announce it. */
  alert?: boolean;
}

/** One row of the project notice list. Every notice renders through it. */
export function NoticeRow({ severity, title, children, actions, alert = false }: NoticeRowProps) {
  const style = ROW_STYLES[severity];
  return (
    <div
      role={alert ? "alert" : undefined}
      className={`flex flex-wrap items-start gap-x-3 gap-y-2 border-t border-l-2 border-t-border-strong/30 px-6 py-2.5 ${style.row}`}
    >
      {style.icon}
      <div className="flex-1 min-w-0 basis-64">
        <div className={`text-[12px] font-semibold ${style.title}`}>
          <span className="sr-only">{SEVERITY_LABELS[severity]}: </span>
          {title}
        </div>
        {children && <div className="mt-0.5 text-[11px] leading-snug text-text-muted">{children}</div>}
      </div>
      {actions && <div className="flex flex-wrap items-center justify-end gap-2 shrink-0">{actions}</div>}
    </div>
  );
}

/** Button styles shared by the rows, so every notice's actions look alike. */
// Solid, not a tint: a brand tint on the warning or danger row tint is too
// dark to read in the dark themes. `text-brand-text` is each theme's own
// colour for text on the brand fill.
export const NOTICE_PRIMARY_BUTTON =
  "flex items-center gap-1.5 px-2.5 py-1 text-[12px] font-medium rounded bg-brand hover:bg-brand-hover text-brand-text shadow-sm transition-colors disabled:opacity-50 disabled:cursor-not-allowed";
export const NOTICE_SECONDARY_BUTTON =
  "flex items-center gap-1.5 px-2.5 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors disabled:opacity-50 disabled:cursor-not-allowed";
export const NOTICE_DANGER_BUTTON =
  "flex items-center gap-1.5 px-2.5 py-1 text-[12px] font-medium rounded border border-danger/40 bg-danger/5 text-danger hover:bg-danger/15 transition-colors disabled:opacity-50";

/** A file or resource chip inside a row. Clickable when `onClick` is set. */
export function NoticeChip({ children, onClick, title }: { children: ReactNode; onClick?: () => void; title?: string }) {
  const className =
    "flex items-center gap-1 text-[11px] font-mono text-text-muted bg-bg-input border border-border-strong/50 rounded px-1.5 py-0.5";
  if (!onClick) return <span className={className}>{children}</span>;
  return (
    <button type="button" onClick={onClick} title={title} className={`${className} hover:text-text-base hover:border-border-strong transition-colors`}>
      {children}
    </button>
  );
}
