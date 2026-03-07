/**
 * TaskLog — floating progress/log panel.
 *
 * Slides up from the bottom of the window while tasks are running.
 * Driven entirely by TaskLogContext — mount this once in App.tsx.
 */
import { useEffect, useRef } from "react";
import { X, CheckCircle, XCircle, Info, Loader } from "lucide-react";
import { useTaskLog, TaskLogStatus } from "./TaskLogContext";

// ── Status indicator ─────────────────────────────────────────────────────────

interface StatusIconProps {
  status: TaskLogStatus;
}

function StatusIcon({ status }: StatusIconProps) {
  switch (status) {
    case "running":
      return <Loader size={12} className="text-brand animate-spin flex-shrink-0" />;
    case "success":
      return <CheckCircle size={12} className="text-success flex-shrink-0" />;
    case "error":
      return <XCircle size={12} className="text-danger flex-shrink-0" />;
    case "info":
    default:
      return <Info size={12} className="text-text-muted flex-shrink-0" />;
  }
}

// ── Timestamp ────────────────────────────────────────────────────────────────

function formatTime(d: Date): string {
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

// ── Panel ────────────────────────────────────────────────────────────────────

export default function TaskLog() {
  const { entries, isVisible, dismiss } = useTaskLog();
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to the bottom when new entries arrive.
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries]);

  // Render nothing (but keep in DOM for transition) — slide controlled via CSS.
  const hasEntries = entries.length > 0;

  return (
    <div
      className={[
        "fixed bottom-0 left-0 right-0 z-50",
        "flex flex-col",
        "transition-transform duration-300 ease-in-out",
        isVisible && hasEntries ? "translate-y-0" : "translate-y-full",
      ].join(" ")}
      aria-live="polite"
      aria-label="Task progress log"
    >
      {/* Panel card — sits above the window bottom edge with a gap */}
      <div className="mx-4 mb-4 bg-bg-sidebar border border-border-strong/60 rounded-xl shadow-2xl overflow-hidden">
        {/* Header bar */}
        <div className="flex items-center justify-between px-4 py-2 border-b border-border-strong/40 bg-bg-input/60">
          <span className="text-[11px] font-semibold tracking-wider uppercase text-text-muted">
            Task Log
          </span>
          <button
            onClick={dismiss}
            className="text-text-muted hover:text-text-base transition-colors rounded p-0.5"
            aria-label="Dismiss task log"
          >
            <X size={12} />
          </button>
        </div>

        {/* Log entries — max height with scroll */}
        <div
          ref={scrollRef}
          className="max-h-40 overflow-y-auto px-4 py-2 flex flex-col gap-1.5"
        >
          {entries.map((entry) => (
            <div key={entry.id} className="flex items-start gap-2">
              <span className="text-[10px] text-text-muted font-mono flex-shrink-0 mt-px leading-4">
                {formatTime(entry.timestamp)}
              </span>
              <StatusIcon status={entry.status} />
              <span
                className={[
                  "text-[12px] leading-4",
                  entry.status === "error" ? "text-danger" : "text-text-base",
                ].join(" ")}
              >
                {entry.message}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
