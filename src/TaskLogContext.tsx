/**
 * TaskLogContext — generic progress/log panel context.
 *
 * Any component can call `useTaskLog()` to push timestamped entries into
 * a shared log that drives the floating TaskLog panel rendered in App.tsx.
 *
 * Entry statuses:
 *   "running"  — spinner, yellow dot
 *   "success"  — green dot
 *   "error"    — red dot
 *   "info"     — grey dot (default)
 */
import { createContext, useCallback, useContext, useRef, useState, ReactNode } from "react";

export type TaskLogStatus = "running" | "success" | "error" | "info";

export interface TaskLogEntry {
  id: string;
  timestamp: Date;
  message: string;
  status: TaskLogStatus;
}

interface TaskLogContextValue {
  entries: TaskLogEntry[];
  isVisible: boolean;
  /** Push a new log entry; returns its generated id. */
  log: (message: string, status?: TaskLogStatus) => string;
  /** Update an existing entry by id (e.g. change status from running → success). */
  update: (id: string, message: string, status: TaskLogStatus) => void;
  /** Clear all entries and hide the panel. */
  clear: () => void;
  /** Manually dismiss the panel (entries are preserved until next clear). */
  dismiss: () => void;
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

export function TaskLogProvider({ children, autoDismissDelay = 4000 }: TaskLogProviderProps) {
  const [entries, setEntries] = useState<TaskLogEntry[]>([]);
  const [isVisible, setIsVisible] = useState(false);
  const dismissTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  /** Reset any pending auto-dismiss timer. */
  const cancelDismiss = useCallback(() => {
    if (dismissTimer.current !== null) {
      clearTimeout(dismissTimer.current);
      dismissTimer.current = null;
    }
  }, []);

  /**
   * Schedule auto-dismiss after `autoDismissDelay` ms, but only when no entry
   * is currently in "running" status.
   */
  const scheduleDismissIfIdle = useCallback(
    (currentEntries: TaskLogEntry[]) => {
      const hasRunning = currentEntries.some((e) => e.status === "running");
      if (hasRunning) return; // still busy — don't dismiss

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
        return next;
      });
    },
    [cancelDismiss, scheduleDismissIfIdle]
  );

  const clear = useCallback(() => {
    cancelDismiss();
    setEntries([]);
    setIsVisible(false);
  }, [cancelDismiss]);

  const dismiss = useCallback(() => {
    cancelDismiss();
    setIsVisible(false);
  }, [cancelDismiss]);

  return (
    <TaskLogContext.Provider value={{ entries, isVisible, log, update, clear, dismiss }}>
      {children}
    </TaskLogContext.Provider>
  );
}
