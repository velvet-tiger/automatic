import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { invokeMock, resetInvokeMock } from "../../../../test/tauriMock";
import type { GitActivity } from "../types";
import { clearGitActivityCache, gitStateFromReply, splitFixCommand, useProjectGitActivity } from "./useProjectGitActivity";

describe("splitFixCommand", () => {
  it("takes git's safe.directory command off the last line", () => {
    expect(splitFixCommand("Git will not read this.\ngit config --global --add safe.directory /srv/app")).toEqual({
      message: "Git will not read this.",
      command: "git config --global --add safe.directory /srv/app",
    });
  });

  it("leaves every other error whole", () => {
    for (const error of [
      "git log failed in /work: boom",
      "git config --global --add safe.directory /srv/app",
      "first line\nsecond line",
    ]) {
      expect(splitFixCommand(error)).toEqual({ message: error, command: null });
    }
  });
});

function activity(subject: string): GitActivity {
  return {
    repo: { branch: "main", uncommitted_files: 0, upstream: null, ahead: null, behind: null, has_commits: true },
    commits: [
      {
        hash: subject,
        timestamp: "2026-11-01T00:00:00Z",
        author: "Sam",
        subject,
        insertions: 1,
        deletions: 0,
        touches_agent_config: false,
      },
    ],
    truncated: false,
    history_days: 98,
    line_stats: true,
  };
}

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const gitCalls = () => invokeMock.mock.calls.filter(([cmd]) => cmd === "get_project_git_activity");

function subjectOf(state: ReturnType<typeof useProjectGitActivity>): string | null {
  return state.status === "loaded" ? state.activity.commits[0].subject : null;
}

describe("useProjectGitActivity", () => {
  beforeEach(() => {
    clearGitActivityCache();
    resetInvokeMock();
    vi.spyOn(console, "error").mockImplementation(() => undefined);
  });

  it("reads nothing and reports none while a project is being created", () => {
    const { result } = renderHook(() => useProjectGitActivity(null, undefined));
    expect(result.current).toEqual({ status: "none" });
    expect(gitCalls()).toHaveLength(0);
  });

  it("loads by local key and reports loading until the reply", async () => {
    const reply = deferred<GitActivity>();
    invokeMock.mockReturnValueOnce(reply.promise);
    const { result } = renderHook(() => useProjectGitActivity("key-a", undefined));
    expect(result.current).toEqual({ status: "loading" });
    expect(invokeMock).toHaveBeenCalledWith("get_project_git_activity", { project: "key-a" });
    await act(async () => reply.resolve(activity("one")));
    expect(subjectOf(result.current)).toBe("one");
  });

  it("never shows one project's reply under another", async () => {
    const slowA = deferred<GitActivity>();
    invokeMock.mockReturnValueOnce(slowA.promise);
    invokeMock.mockResolvedValueOnce(activity("from b"));
    const { result, rerender } = renderHook(({ key }) => useProjectGitActivity(key, undefined), {
      initialProps: { key: "key-a" },
    });
    rerender({ key: "key-b" });
    expect(result.current).toEqual({ status: "loading" });
    await act(async () => slowA.resolve(activity("from a")));
    await waitFor(() => expect(subjectOf(result.current)).toBe("from b"));
    expect(gitCalls().map(([, args]) => args)).toEqual([{ project: "key-a" }, { project: "key-b" }]);
  });

  it("collapses a burst of focus events into one more read", async () => {
    const first = deferred<GitActivity>();
    invokeMock.mockReturnValueOnce(first.promise);
    invokeMock.mockResolvedValue(activity("second"));
    const { result } = renderHook(() => useProjectGitActivity("key-a", undefined));
    act(() => {
      for (let i = 0; i < 5; i += 1) window.dispatchEvent(new Event("focus"));
    });
    expect(gitCalls()).toHaveLength(1);
    await act(async () => first.resolve(activity("first")));
    await waitFor(() => expect(subjectOf(result.current)).toBe("second"));
    expect(gitCalls()).toHaveLength(2);
  });

  it("reloads after a save and keeps the current data on screen meanwhile", async () => {
    invokeMock.mockResolvedValueOnce(activity("before"));
    const { result, rerender } = renderHook(({ refresh }) => useProjectGitActivity("key-a", refresh), {
      initialProps: { refresh: 1 },
    });
    await waitFor(() => expect(subjectOf(result.current)).toBe("before"));
    const after = deferred<GitActivity>();
    invokeMock.mockReturnValueOnce(after.promise);
    rerender({ refresh: 2 });
    expect(subjectOf(result.current)).toBe("before");
    await act(async () => after.resolve(activity("after")));
    expect(subjectOf(result.current)).toBe("after");
  });

  it("does not start a second read when the Summary is remounted mid-read", async () => {
    const slow = deferred<GitActivity>();
    invokeMock.mockReturnValueOnce(slow.promise);
    invokeMock.mockResolvedValue(activity("follow-up"));
    const first = renderHook(() => useProjectGitActivity("key-a", undefined));
    first.unmount();
    const second = renderHook(() => useProjectGitActivity("key-a", undefined));
    expect(gitCalls()).toHaveLength(1);
    await act(async () => slow.resolve(activity("slow")));
    await waitFor(() => expect(subjectOf(second.result.current)).toBe("follow-up"));
    expect(gitCalls()).toHaveLength(2);
  });

  it("shows the cached result when a project is opened again", async () => {
    invokeMock.mockResolvedValueOnce(activity("cached"));
    const first = renderHook(() => useProjectGitActivity("key-a", undefined));
    await waitFor(() => expect(subjectOf(first.result.current)).toBe("cached"));
    first.unmount();
    invokeMock.mockReturnValueOnce(new Promise(() => undefined));
    const second = renderHook(() => useProjectGitActivity("key-a", undefined));
    expect(subjectOf(second.result.current)).toBe("cached");
  });

  it("reports a failed read with its reason", async () => {
    invokeMock.mockRejectedValueOnce("git status failed in /x: boom");
    const { result } = renderHook(() => useProjectGitActivity("key-a", undefined));
    await waitFor(() => expect(result.current).toEqual({ status: "failed", error: "git status failed in /x: boom" }));
  });
});

describe("gitStateFromReply", () => {
  it("maps null to none and checks the shape of anything else", () => {
    expect(gitStateFromReply(null)).toEqual({ status: "none" });
    expect(gitStateFromReply(activity("x")).status).toBe("loaded");
    expect(gitStateFromReply(undefined).status).toBe("failed");
    expect(gitStateFromReply({ commits: [] }).status).toBe("failed");
  });
});
