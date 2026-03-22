/**
 * TaskLogContext — generic progress/log panel context.
 *
 * Any component can call `useTaskLog()` to push timestamped entries into
 * a shared log that drives the floating TaskLog panel rendered in App.tsx.
 *
 * Entries are persisted to disk via the `get_task_log` / `append_task_log`
 * Tauri commands so they survive app restarts. The last 500 entries are kept.
 *
 * Entry statuses:
 *   "running"  — spinner, yellow dot
 *   "success"  — green dot
 *   "error"    — red dot
 *   "info"     — grey dot (default)
 */
import { createContext, useCallback, useContext, useEffect, useRef, useState, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

export type TaskLogStatus = "running" | "success" | "error" | "info";

export interface TaskLogEntry {
  id: string;
  timestamp: Date;
  message: string;
  status: TaskLogStatus;
}

// Shape expected by the Rust backend (Date → ISO string).
interface PersistedEntry {
  id: string;
  timestamp: string;
  message: string;
  status: string;
}

interface TaskLogContextValue {
  entries: TaskLogEntry[];
  isVisible: boolean;
  /**
   * True when the panel was opened via the manual toggle button.
   * When true, auto-dismiss is suppressed and the user must close it explicitly.
   */
  isManuallyOpened: boolean;
  /** Push a new log entry; returns its generated id. */
  log: (message: string, status?: TaskLogStatus) => string;
  /** Update an existing entry by id (e.g. change status from running → success). */
  update: (id: string, message: string, status: TaskLogStatus) => void;
  /** Clear all entries and hide the panel. */
  clear: () => void;
  /** Manually dismiss the panel (entries are preserved until next clear). */
  dismiss: () => void;
  /** Force the panel open, suppressing auto-dismiss until the user closes it. */
  show: () => void;
}

const TaskLogContext = createContext<TaskLogContextValue | null>(null);

export function useTaskLog(): TaskLogContextValue {
  const ctx = useContext(TaskLogContext);
  if (!ctx) throw new Error("useTaskLog must be used inside <TaskLogProvider>");
  return ctx;
}

interface TaskLogProviderProps {
  children: ReactNode;
  /** Milliseconds of idle time after all tasks finish before auto-dismissing. Default 4000. */
  autoDismissDelay?: number;
}

// ── Persistence helpers ───────────────────────────────────────────────────────

function toPersistedEntry(entry: TaskLogEntry): PersistedEntry {
  return {
    id: entry.id,
    timestamp: entry.timestamp.toISOString(),
    message: entry.message,
    status: entry.status,
  };
}

function fromPersistedEntry(p: PersistedEntry): TaskLogEntry {
  return {
    id: p.id,
    timestamp: new Date(p.timestamp),
    message: p.message,
    status: (p.status as TaskLogStatus) ?? "info",
  };
}

/** Fire-and-forget persist call — errors are logged but never surface to the UI. */
function persistEntries(entries: PersistedEntry[]): void {
  invoke("append_task_log", { entries }).catch((e) => {
    console.error("[task-log] Failed to persist entries:", e);
  });
}

// ── Provider ──────────────────────────────────────────────────────────────────

export function TaskLogProvider({ children, autoDismissDelay = 4000 }: TaskLogProviderProps) {
  const [entries, setEntries] = useState<TaskLogEntry[]>([]);
  const [isVisible, setIsVisible] = useState(false);
  const [isManuallyOpened, setIsManuallyOpened] = useState(false);
  const dismissTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isManuallyOpenedRef = useRef(false);

  // ── Load persisted entries on mount ────────────────────────────────────────
  useEffect(() => {
    invoke<PersistedEntry[]>("get_task_log")
      .then((persisted) => {
        if (persisted.length > 0) {
          setEntries(persisted.map(fromPersistedEntry));
        }
      })
      .catch((e) => {
        console.error("[task-log] Failed to load persisted entries:", e);
      });
  }, []);

  /** Reset any pending auto-dismiss timer. */
  const cancelDismiss = useCallback(() => {
    if (dismissTimer.current !== null) {
      clearTimeout(dismissTimer.current);
      dismissTimer.current = null;
    }
  }, []);

  /**
   * Schedule auto-dismiss after `autoDismissDelay` ms, but only when no entry
   * is currently in "running" status and the panel was not manually opened.
   */
  const scheduleDismissIfIdle = useCallback(
    (currentEntries: TaskLogEntry[]) => {
      const hasRunning = currentEntries.some((e) => e.status === "running");
      if (hasRunning) return; // still busy — don't dismiss
      if (isManuallyOpenedRef.current) return; // user opened it manually — don't auto-dismiss

      cancelDismiss();
      dismissTimer.current = setTimeout(() => {
        setIsVisible(false);
      }, autoDismissDelay);
    },
    [autoDismissDelay, cancelDismiss]
  );

  const log = useCallback(
    (message: string, status: TaskLogStatus = "info"): string => {
      const id = `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
      const entry: TaskLogEntry = { id, timestamp: new Date(), message, status };

      cancelDismiss(); // opening a new entry cancels any pending auto-dismiss
      setIsVisible(true);
      setEntries((prev) => {
        const next = [...prev, entry];
        if (status !== "running") scheduleDismissIfIdle(next);
        persistEntries([toPersistedEntry(entry)]);
        return next;
      });

      return id;
    },
    [cancelDismiss, scheduleDismissIfIdle]
  );

  const update = useCallback(
    (id: string, message: string, status: TaskLogStatus) => {
      cancelDismiss();
      setEntries((prev) => {
        const next = prev.map((e) => (e.id === id ? { ...e, message, status, timestamp: new Date() } : e));
        scheduleDismissIfIdle(next);
        const updated = next.find((e) => e.id === id);
        if (updated) persistEntries([toPersistedEntry(updated)]);
        return next;
      });
    },
    [cancelDismiss, scheduleDismissIfIdle]
  );

  const clear = useCallback(() => {
    cancelDismiss();
    isManuallyOpenedRef.current = false;
    setIsManuallyOpened(false);
    setEntries([]);
    setIsVisible(false);
  }, [cancelDismiss]);

  const dismiss = useCallback(() => {
    cancelDismiss();
    isManuallyOpenedRef.current = false;
    setIsManuallyOpened(false);
    setIsVisible(false);
  }, [cancelDismiss]);

  const show = useCallback(() => {
    cancelDismiss();
    isManuallyOpenedRef.current = true;
    setIsManuallyOpened(true);
    setIsVisible(true);
  }, [cancelDismiss]);

  return (
    <TaskLogContext.Provider value={{ entries, isVisible, isManuallyOpened, log, update, clear, dismiss, show }}>
      {children}
    </TaskLogContext.Provider>
  );
}
