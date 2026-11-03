import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock } from "../test/tauriMock";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { TaskLogProvider } from "../contexts/TaskLogContext";
import { UpdateProvider } from "../contexts/UpdateContext";
import Settings from "./Settings";

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.0.0-test"),
}));

const LATEST_RELEASE_URL = "https://github.com/velvet-tiger/automatic/releases/latest";
const NEW_ISSUE_URL = "https://github.com/velvet-tiger/automatic/issues/new";

describe("Settings > App > App Updates", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({});
    vi.mocked(openUrl).mockClear();
  });

  it("opens the latest release in the system browser", async () => {
    render(
      <TaskLogProvider>
        <UpdateProvider>
          <Settings initialPage="app" />
        </UpdateProvider>
      </TaskLogProvider>,
    );

    const link = await screen.findByRole("link", { name: "View changelog" });
    expect(link).toHaveAttribute("href", LATEST_RELEASE_URL);

    fireEvent.click(link);
    await waitFor(() => {
      expect(openUrl).toHaveBeenCalledWith(LATEST_RELEASE_URL);
    });
  });

  it("opens the new issue form in the system browser", async () => {
    render(
      <TaskLogProvider>
        <UpdateProvider>
          <Settings initialPage="app" />
        </UpdateProvider>
      </TaskLogProvider>,
    );

    const link = await screen.findByRole("link", { name: "Report an issue" });
    expect(link).toHaveAttribute("href", NEW_ISSUE_URL);

    fireEvent.click(link);
    await waitFor(() => {
      expect(openUrl).toHaveBeenCalledWith(NEW_ISSUE_URL);
    });
  });
});
