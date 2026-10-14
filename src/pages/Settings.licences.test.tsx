import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock } from "../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { TaskLogProvider } from "../contexts/TaskLogContext";
import { UpdateProvider } from "../contexts/UpdateContext";
import Settings from "./Settings";

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.0.0-test"),
}));

describe("Settings > Support > Open-source licences", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({
      read_third_party_licences: {
        folder: "/licenses",
        files: [{ file_name: "rust-crates.txt", text: "crate licences" }],
      },
    });
  });

  it("opens the licences dialog from the Support page", async () => {
    render(
      <TaskLogProvider>
        <UpdateProvider>
          <Settings initialPage="support" />
        </UpdateProvider>
      </TaskLogProvider>,
    );
    fireEvent.click(await screen.findByRole("button", { name: /Open-source licences/i }));
    await waitFor(() => {
      expect(screen.getByRole("dialog", { name: "Open-source licences" })).toBeInTheDocument();
    });
    expect(await screen.findByText("crate licences")).toBeInTheDocument();
  });
});
