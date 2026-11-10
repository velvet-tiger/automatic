import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { WritableFoldersTargetNotice, targetReasonText } from "./WritableFoldersTargetNotice";
import type { UserSettingsReason, WritableFoldersTargetNotice as TargetNotice } from "../../types";

const notice: TargetNotice = {
  folders: ["/Users/me/work/cli", "/Users/me/work/meta"],
  reason: "tracked",
  paste_json: "{}",
};

function renderNotice(overrides: Partial<Parameters<typeof WritableFoldersTargetNotice>[0]> = {}) {
  const props = {
    notice,
    selectedKey: "app",
    reloadProject: vi.fn().mockResolvedValue(undefined),
    refreshProblems: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  render(<WritableFoldersTargetNotice {...props} />);
  return props;
}

describe("WritableFoldersTargetNotice", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({
      set_writable_folders_user_consent: null,
      sync_project_writable_folders: { target: null, wanted: [], errors: [] },
      set_writable_folders_notice_hidden: null,
    });
  });

  it("explains the reason and counts the folders, without paths", () => {
    renderNotice();
    expect(screen.getByText("Claude Code needs your OK to save this project's folders")).toBeInTheDocument();
    expect(screen.getByText(/because they are committed to git\./)).toBeInTheDocument();
    expect(screen.getByText(/save the 2 folders for all your Claude Code sessions/)).toBeInTheDocument();
    expect(screen.getByText(/any other project whose own settings can't be used, without asking again/)).toBeInTheDocument();
    expect(screen.queryByText(/\/Users\/me/)).toBeNull();
  });

  it("records consent, syncs this project, then checks again", async () => {
    const props = renderNotice();
    fireEvent.click(screen.getByText("Save for all my sessions"));
    await waitFor(() => expect(props.refreshProblems).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("set_writable_folders_user_consent", { consent: true });
    expect(invokeMock).toHaveBeenCalledWith("sync_project_writable_folders", { name: "app" });
    const order = invokeMock.mock.calls.map((call) => call[0]);
    expect(order.indexOf("set_writable_folders_user_consent")).toBeLessThan(
      order.indexOf("sync_project_writable_folders"),
    );
  });

  it("shows the errors the sync returned", async () => {
    mockInvoke({ sync_project_writable_folders: { target: null, wanted: [], errors: ["disk full"] } });
    const props = renderNotice();
    fireEvent.click(screen.getByText("Save for all my sessions"));
    expect(await screen.findByText(/Could not save the folders for all your sessions: disk full/)).toBeInTheDocument();
    expect(props.refreshProblems).toHaveBeenCalled();
  });

  it("hides the notice for this project on Not now", async () => {
    const props = renderNotice();
    fireEvent.click(screen.getByText("Not now"));
    await waitFor(() => expect(props.refreshProblems).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("set_writable_folders_notice_hidden", { name: "app", hidden: true });
    expect(props.reloadProject).toHaveBeenCalledWith("app");
    expect(invokeMock).not.toHaveBeenCalledWith("set_writable_folders_user_consent", expect.anything());
  });

  it("shows a failure and keeps the buttons", async () => {
    resetInvokeMock();
    mockInvoke({
      set_writable_folders_user_consent: () => {
        throw new Error("disk full");
      },
    });
    renderNotice();
    fireEvent.click(screen.getByText("Save for all my sessions"));
    expect(await screen.findByText(/Could not save the folders for all your sessions: Error: disk full/)).toBeInTheDocument();
    expect(screen.getByText("Save for all my sessions")).not.toBeDisabled();
  });

  it("has a sentence for every reason", () => {
    const reasons: UserSettingsReason[] = [
      "preferred",
      "silent",
      "read_only",
      "not_repository_root",
      "tracked",
      "seen_tracked",
      "git_unavailable",
      "trust_unknown",
    ];
    for (const reason of reasons) expect(targetReasonText(reason)).toMatch(/^because .+\.$/);
  });
});
