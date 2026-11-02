/**
 * Erasing app data can stop or leave values in Claude Code's settings. The
 * modal has to say so rather than only logging it.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock } from "../test/tauriMock";
import { render, screen, fireEvent } from "@testing-library/react";
import { TaskLogProvider } from "../contexts/TaskLogContext";
import { UpdateProvider } from "../contexts/UpdateContext";
import Settings from "./Settings";

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.0.0-test"),
}));

async function confirmErase() {
  render(
    <TaskLogProvider>
      <UpdateProvider>
        <Settings initialPage="app" />
      </UpdateProvider>
    </TaskLogProvider>,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Erase Data" }));
  fireEvent.change(screen.getByPlaceholderText("type erase"), { target: { value: "erase" } });
  const buttons = screen.getAllByRole("button", { name: "Erase Data" });
  fireEvent.click(buttons[buttons.length - 1]);
}

describe("Settings erase app data", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("shows why the erase stopped", async () => {
    mockInvoke({
      erase_app_data: () => Promise.reject("App data was not erased. /home/me/.claude/settings.json"),
    });
    await confirmErase();

    expect(await screen.findByRole("alert")).toHaveTextContent("App data was not erased");
    expect(screen.getByPlaceholderText("type erase")).toBeInTheDocument();
  });

  it("reports what stayed in Claude Code's settings", async () => {
    mockInvoke({ erase_app_data: { left_in_claude_code: ["ANTHROPIC_AUTH_TOKEN"] } });
    await confirmErase();

    expect(await screen.findByText(/ANTHROPIC_AUTH_TOKEN is still set/)).toBeInTheDocument();
  });
});
