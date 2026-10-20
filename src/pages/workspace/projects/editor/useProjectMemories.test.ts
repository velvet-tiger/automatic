import { describe, it, expect, beforeEach, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { invokeMock, resetInvokeMock, type InvokeArgs } from "../../../../test/tauriMock";
import { useProjectMemories } from "./useProjectMemories";
import type { MemoryRecord } from "../../../../components/MemoryBrowser";

interface Deferred {
  resolve: (value: MemoryRecord) => void;
  reject: (reason: unknown) => void;
}

/** Each get_project_memories call waits until the test settles it. */
function deferInvokes(): Map<string, Deferred> {
  const pending = new Map<string, Deferred>();
  invokeMock.mockImplementation((cmd: string, args: InvokeArgs) => {
    if (cmd !== "get_project_memories") return Promise.resolve(undefined);
    return new Promise<MemoryRecord>((resolve, reject) => {
      pending.set(String(args?.project), { resolve, reject });
    });
  });
  return pending;
}

function entry(value: string) {
  return { value, timestamp: "2026-10-20T00:00:00Z", source: null };
}

function settle(pending: Map<string, Deferred>, key: string): Deferred {
  const d = pending.get(key);
  if (!d) throw new Error(`no pending load for ${key}`);
  return d;
}

describe("useProjectMemories", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("ignores a slow earlier load that finishes after the latest one", async () => {
    const pending = deferInvokes();
    const onError = vi.fn();
    const { result, rerender } = renderHook(
      ({ selected }: { selected: string }) => useProjectMemories(selected, onError),
      { initialProps: { selected: "alpha" } },
    );

    let loadA: Promise<void> = Promise.resolve();
    let loadB: Promise<void> = Promise.resolve();
    act(() => {
      loadA = result.current.loadMemories("alpha");
    });
    rerender({ selected: "beta" });
    act(() => {
      loadB = result.current.loadMemories("beta");
    });

    await act(async () => {
      settle(pending, "beta").resolve({ b: entry("beta value") });
      await loadB;
    });
    expect(result.current.selectedMemories).toEqual({ b: entry("beta value") });
    expect(result.current.loadingMemories).toBe(false);

    await act(async () => {
      settle(pending, "alpha").resolve({ a: entry("alpha value") });
      await loadA;
    });
    expect(result.current.selectedMemories).toEqual({ b: entry("beta value") });
    expect(result.current.loadingMemories).toBe(false);
  });

  it("keeps the loading flag on until the latest load finishes", async () => {
    const pending = deferInvokes();
    const { result } = renderHook(() => useProjectMemories("beta", vi.fn()));

    let loadA: Promise<void> = Promise.resolve();
    act(() => {
      loadA = result.current.loadMemories("alpha");
    });
    act(() => {
      void result.current.loadMemories("beta");
    });

    await act(async () => {
      settle(pending, "alpha").reject("slow failure");
      await loadA;
    });
    expect(result.current.loadingMemories).toBe(true);
  });

  it("drops a stale load's error", async () => {
    const pending = deferInvokes();
    const onError = vi.fn();
    const { result } = renderHook(() => useProjectMemories("beta", onError));

    let loadA: Promise<void> = Promise.resolve();
    let loadB: Promise<void> = Promise.resolve();
    act(() => {
      loadA = result.current.loadMemories("alpha");
    });
    act(() => {
      loadB = result.current.loadMemories("beta");
    });
    await act(async () => {
      settle(pending, "beta").resolve({ b: entry("beta value") });
      await loadB;
      settle(pending, "alpha").reject("old failure");
      await loadA;
    });

    expect(onError).not.toHaveBeenCalled();
    expect(result.current.selectedMemories).toEqual({ b: entry("beta value") });
  });

  it("clears the selected project's memories when its load fails", async () => {
    const pending = deferInvokes();
    const onError = vi.fn();
    const { result } = renderHook(() => useProjectMemories("alpha", onError));

    let load: Promise<void> = Promise.resolve();
    act(() => {
      load = result.current.loadMemories("alpha");
    });
    await act(async () => {
      settle(pending, "alpha").resolve({ a: entry("v") });
      await load;
    });
    act(() => {
      load = result.current.loadMemories("alpha");
    });
    await act(async () => {
      settle(pending, "alpha").reject("disk error");
      await load;
    });

    expect(result.current.selectedMemories).toBeNull();
    expect(onError).toHaveBeenCalledWith("Failed to load memories: disk error");
  });
});
