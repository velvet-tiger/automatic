import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { invokeMock, mockInvoke, resetInvokeMock } from "../../../../test/tauriMock";
import { useProjectActivity } from "./useProjectActivity";

const entry = (id: number) => ({
  id,
  project: "a",
  event: "sync",
  label: "Synced",
  detail: "",
  timestamp: "2026-11-01T00:00:00Z",
});

function deferred() {
  let resolve: (value: string) => void = () => undefined;
  const promise = new Promise<string>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("useProjectActivity", () => {
  beforeEach(() => {
    resetInvokeMock();
    vi.spyOn(console, "error").mockImplementation(() => undefined);
  });

  it("reports loading from selection until the reply arrives", async () => {
    mockInvoke({ get_project_activity: JSON.stringify([entry(1)]) });
    const { result } = renderHook(() => useProjectActivity("a"));
    expect(result.current.loadingActivity).toBe(true);
    await act(() => result.current.loadActivity("a"));
    expect(result.current.loadingActivity).toBe(false);
    expect(result.current.activityEntries).toHaveLength(1);
  });

  it("hides entries that belong to another project", async () => {
    mockInvoke({ get_project_activity: JSON.stringify([entry(1)]) });
    const { result, rerender } = renderHook(({ key }) => useProjectActivity(key), {
      initialProps: { key: "a" },
    });
    await act(() => result.current.loadActivity("a"));
    rerender({ key: "b" });
    expect(result.current.activityEntries).toHaveLength(0);
    expect(result.current.loadingActivity).toBe(true);
  });

  it("drops a slow reply once a newer request has started", async () => {
    const slow = deferred();
    invokeMock.mockReturnValueOnce(slow.promise);
    invokeMock.mockResolvedValueOnce(JSON.stringify([entry(2)]));
    const { result, rerender } = renderHook(({ key }) => useProjectActivity(key), {
      initialProps: { key: "a" },
    });
    let first: Promise<void> = Promise.resolve();
    act(() => {
      first = result.current.loadActivity("a");
    });
    rerender({ key: "b" });
    await act(() => result.current.loadActivity("b"));
    slow.resolve(JSON.stringify([entry(1)]));
    await act(() => first);
    expect(result.current.activityEntries.map((item) => item.id)).toEqual([2]);
  });

  it("ignores a late load for a project that is no longer selected", async () => {
    invokeMock.mockResolvedValueOnce(JSON.stringify([entry(2)]));
    const { result, rerender } = renderHook(({ key }) => useProjectActivity(key), {
      initialProps: { key: "a" },
    });
    rerender({ key: "b" });
    await act(() => result.current.loadActivity("b"));
    await act(() => result.current.loadActivity("a"));
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(result.current.loadingActivity).toBe(false);
    expect(result.current.activityEntries.map((item) => item.id)).toEqual([2]);
  });

  it("keeps entries on screen during a refresh", async () => {
    const refresh = deferred();
    invokeMock.mockResolvedValueOnce(JSON.stringify([entry(1)]));
    invokeMock.mockReturnValueOnce(refresh.promise);
    const { result } = renderHook(() => useProjectActivity("a"));
    await act(() => result.current.loadActivity("a"));
    let pending: Promise<void> = Promise.resolve();
    act(() => {
      pending = result.current.loadActivity("a");
    });
    expect(result.current.loadingActivity).toBe(false);
    expect(result.current.activityEntries).toHaveLength(1);
    refresh.resolve("[]");
    await act(() => pending);
    expect(result.current.activityEntries).toHaveLength(0);
  });

  it("drops old entries when a refresh fails", async () => {
    invokeMock.mockResolvedValueOnce(JSON.stringify([entry(1)]));
    invokeMock.mockRejectedValueOnce(new Error("locked"));
    const { result } = renderHook(() => useProjectActivity("a"));
    await act(() => result.current.loadActivity("a"));
    await act(() => result.current.loadActivity("a"));
    await waitFor(() => expect(result.current.activityLoadFailed).toBe(true));
    expect(result.current.activityEntries).toHaveLength(0);
    expect(result.current.loadingActivity).toBe(false);
  });
});
