import { useCallback, useEffect, useRef, useState } from "react";

export type SaveStatus =
  | { kind: "idle" }
  | { kind: "pending" }
  | { kind: "saving" }
  | { kind: "saved" }
  | { kind: "error"; message: string };

/**
 * Save the latest value after a pause, one save at a time and in order, so
 * a slow save can never land after a newer one. `flush` saves at once and
 * resolves when every scheduled value is on disk; call it before switching
 * away from what is being edited. Unmounting flushes too.
 */
export function useAutosave<T>(save: (value: T) => Promise<void>, delayMs: number) {
  const [status, setStatus] = useState<SaveStatus>({ kind: "idle" });
  const saveRef = useRef(save);
  saveRef.current = save;
  const pending = useRef<{ value: T } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const chain = useRef<Promise<void>>(Promise.resolve());
  const mounted = useRef(true);
  const lastError = useRef<unknown>(null);

  const report = (next: SaveStatus) => {
    if (mounted.current) setStatus(next);
  };

  const flush = useCallback((): Promise<void> => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    chain.current = chain.current.then(async () => {
      const next = pending.current;
      if (!next) return;
      pending.current = null;
      report({ kind: "saving" });
      try {
        await saveRef.current(next.value);
        lastError.current = null;
        report(pending.current ? { kind: "pending" } : { kind: "saved" });
      } catch (err) {
        lastError.current = err;
        report({ kind: "error", message: String(err) });
      }
    });
    return chain.current;
  }, []);

  const schedule = useCallback(
    (value: T, delay: number = delayMs) => {
      pending.current = { value };
      report({ kind: "pending" });
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
      if (delay <= 0) void flush();
      else timer.current = setTimeout(() => void flush(), delay);
    },
    [delayMs, flush],
  );

  /** Like `flush`, but rejects when the last save failed. */
  const flushOrThrow = useCallback(async (): Promise<void> => {
    await flush();
    if (lastError.current) throw lastError.current;
  }, [flush]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      void flush();
    };
  }, [flush]);

  return { status, schedule, flush, flushOrThrow };
}

export function describeSaveStatus(status: SaveStatus): string {
  switch (status.kind) {
    case "idle":
      return "";
    case "pending":
    case "saving":
      return "Saving…";
    case "saved":
      return "All changes saved";
    case "error":
      return `Couldn't save: ${status.message}`;
  }
}
