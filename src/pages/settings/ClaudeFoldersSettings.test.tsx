import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock, type InvokeArgs } from "../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { ask } from "@tauri-apps/plugin-dialog";
import { ClaudeFoldersSettings, parseFolderSettings } from "./ClaudeFoldersSettings";

function withSettings(settings: Record<string, unknown>) {
  mockInvoke({
    read_settings: () => settings,
    set_writable_folders_target: (args: InvokeArgs) => {
      settings.writable_folders_target = (args as { target: string }).target;
      if (settings.writable_folders_target === "always_user") settings.writable_folders_user_consent = true;
      return null;
    },
    set_writable_folders_user_consent: (args: InvokeArgs) => {
      settings.writable_folders_user_consent = (args as { consent: boolean }).consent;
      if (!settings.writable_folders_user_consent) settings.writable_folders_target = "prefer_local";
      return null;
    },
  });
}

describe("parseFolderSettings", () => {
  it("defaults anything unexpected", () => {
    expect(parseFolderSettings({})).toEqual({ target: "prefer_local", consent: false });
    expect(parseFolderSettings({ writable_folders_target: "always_user", writable_folders_user_consent: true })).toEqual({
      target: "always_user",
      consent: true,
    });
  });
});

describe("ClaudeFoldersSettings", () => {
  beforeEach(() => {
    resetInvokeMock();
    vi.mocked(ask).mockReset();
  });

  it("asks before choosing user settings without consent, and does nothing on Cancel", async () => {
    vi.mocked(ask).mockResolvedValue(false);
    withSettings({});
    render(<ClaudeFoldersSettings />);
    fireEvent.click(await screen.findByRole("radio", { name: /For all my Claude Code sessions/ }));
    await waitFor(() => expect(ask).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("set_writable_folders_target", expect.anything());
  });

  it("does not ask again when consent is already given", async () => {
    withSettings({ writable_folders_user_consent: true });
    render(<ClaudeFoldersSettings />);
    fireEvent.click(await screen.findByRole("radio", { name: /For all my Claude Code sessions/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_writable_folders_target", { target: "always_user" }));
    expect(ask).not.toHaveBeenCalled();
  });

  it("chooses user settings for every project, which also turns consent on", async () => {
    vi.mocked(ask).mockResolvedValue(true);
    withSettings({});
    render(<ClaudeFoldersSettings />);
    fireEvent.click(await screen.findByRole("radio", { name: /For all my Claude Code sessions/ }));
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "Let Automatic change my Claude Code settings" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    expect(invokeMock).toHaveBeenCalledWith("set_writable_folders_target", { target: "always_user" });
  });

  it("turns consent on without asking", async () => {
    withSettings({});
    render(<ClaudeFoldersSettings />);
    fireEvent.click(await screen.findByRole("switch"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_writable_folders_user_consent", { consent: true }));
    expect(ask).not.toHaveBeenCalled();
  });

  it("asks before turning consent off, and does nothing on Cancel", async () => {
    vi.mocked(ask).mockResolvedValue(false);
    withSettings({ writable_folders_user_consent: true });
    render(<ClaudeFoldersSettings />);
    fireEvent.click(await screen.findByRole("switch"));
    await waitFor(() => expect(ask).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("set_writable_folders_user_consent", expect.anything());
  });

  it("turns consent off after the confirmation, which resets the choice", async () => {
    vi.mocked(ask).mockResolvedValue(true);
    withSettings({ writable_folders_user_consent: true, writable_folders_target: "always_user" });
    render(<ClaudeFoldersSettings />);
    fireEvent.click(await screen.findByRole("switch"));
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: /In each project/ })).toHaveAttribute("aria-checked", "true"),
    );
    expect(invokeMock).toHaveBeenCalledWith("set_writable_folders_user_consent", { consent: false });
  });
});
