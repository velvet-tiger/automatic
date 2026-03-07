/**
 * TaskLog — floating progress/log panel.
 *
 * Slides up from the bottom of the window while tasks are running.
 * Driven entirely by TaskLogContext — mount this once in App.tsx.
 */
import { useEffect, useRef, useState } from "react";
import { X, CheckCircle, XCircle, Info, Loader, Copy, Check } from "lucide-react";
import { useTaskLog, TaskLogStatus, TaskLogEntry } from "./TaskLogContext";

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

// ── Copy button (single entry) ───────────────────────────────────────────────

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  function handleCopy() {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }

  return (
    <button
      onClick={handleCopy}
      className="opacity-0 group-hover:opacity-100 flex-shrink-0 text-text-muted hover:text-text-base transition-all rounded p-0.5 ml-auto"
      aria-label="Copy log entry"
      title="Copy"
    >
      {copied ? <Check size={11} className="text-success" /> : <Copy size={11} />}
    </button>
  );
}

// ── Copy-all button (header) ─────────────────────────────────────────────────

function CopyAllButton({ entries }: { entries: TaskLogEntry[] }) {
  const [copied, setCopied] = useState(false);

  function handleCopy() {
    const text = entries
      .map((e) => `[${formatTime(e.timestamp)}] [${e.status}] ${e.message}`)
      .join("\n");
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }

  return (
    <button
      onClick={handleCopy}
      className="text-text-muted hover:text-text-base transition-colors rounded p-0.5"
      aria-label="Copy all log entries"
      title="Copy all"
    >
      {copied ? <Check size={12} className="text-success" /> : <Copy size={12} />}
    </button>
  );
}

// ── Panel ────────────────────────────────────────────────────────────────────

export default function TaskLog() {
  const { entries, isVisible, isManuallyOpened, dismiss } = useTaskLog();
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to the bottom when new entries arrive.
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries]);

  // When auto-shown (not manually opened), require at least one entry to be visible.
  // When manually opened, show even if there are no entries yet.
  const shouldShow = isVisible && (isManuallyOpened || entries.length > 0);

  return (
    <div
      className={[
        "fixed bottom-0 left-0 right-0 z-50",
        "flex flex-col",
        "transition-transform duration-300 ease-in-out",
        shouldShow ? "translate-y-0" : "translate-y-full",
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
          <div className="flex items-center gap-1">
            {entries.length > 0 && <CopyAllButton entries={entries} />}
            <button
              onClick={dismiss}
              className="text-text-muted hover:text-text-base transition-colors rounded p-0.5"
              aria-label="Dismiss task log"
            >
              <X size={12} />
            </button>
          </div>
        </div>

        {/* Log entries — 10-line scrollable area */}
        <div
          ref={scrollRef}
          className="max-h-[160px] overflow-y-auto px-4 py-2 flex flex-col gap-1.5"
        >
          {entries.length === 0 ? (
            <span className="text-[12px] text-text-muted italic py-1">No recent activity.</span>
          ) : (
            entries.map((entry) => (
              <div key={entry.id} className="group flex items-start gap-2">
                <span className="text-[10px] text-text-muted font-mono flex-shrink-0 mt-px leading-4">
                  {formatTime(entry.timestamp)}
                </span>
                <StatusIcon status={entry.status} />
                <span
                  className={[
                    "text-[12px] leading-4 flex-1 min-w-0",
                    entry.status === "error" ? "text-danger" : "text-text-base",
                  ].join(" ")}
                >
                  {entry.message}
                </span>
                <CopyButton text={`[${formatTime(entry.timestamp)}] [${entry.status}] ${entry.message}`} />
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
