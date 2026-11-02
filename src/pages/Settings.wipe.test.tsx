/**
 * Reset and Erase can fail on the secret store. A user who confirmed one of
 * them must be told, or it looks as if nothing happened.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock } from "../test/tauriMock";
import { ask, message } from "@tauri-apps/plugin-dialog";
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

const renderSettings = () =>
  render(
    <TaskLogProvider>
      <UpdateProvider>
        <Settings initialPage="app" />
      </UpdateProvider>
    </TaskLogProvider>,
  );

describe("Settings reset and erase failures", () => {
  beforeEach(() => {
    resetInvokeMock();
    vi.mocked(message).mockClear();
    vi.mocked(ask).mockResolvedValue(true);
    mockInvoke({
      reset_settings: () => Promise.reject("the keychain refused"),
      erase_app_data: () => Promise.reject("the keychain refused"),
    });
  });

  it("tells the user when a reset fails", async () => {
    renderSettings();
    fireEvent.click(await screen.findByRole("button", { name: "Reset to Factory Settings" }));

    await waitFor(() => expect(message).toHaveBeenCalledTimes(1));
    expect(vi.mocked(message).mock.calls[0][0]).toContain("the keychain refused");
  });

  it("tells the user when an erase fails", async () => {
    renderSettings();
    fireEvent.click(await screen.findByRole("button", { name: "Erase Data" }));
    fireEvent.change(screen.getByPlaceholderText("type erase"), { target: { value: "erase" } });
    const [, confirm] = screen.getAllByRole("button", { name: "Erase Data" });
    fireEvent.click(confirm);

    await waitFor(() => expect(message).toHaveBeenCalledTimes(1));
    expect(vi.mocked(message).mock.calls[0][0]).toContain("the keychain refused");
  });
});
