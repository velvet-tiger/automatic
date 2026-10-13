/**
 * The Agents settings page must not offer to add a key when it could not
 * find out whether one is stored. Adding a key replaces the stored one.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock } from "../../test/tauriMock";
import { render, screen } from "@testing-library/react";
import SettingsAgents from "./Agents";

describe("Agents settings key status", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("offers to add a key when none is stored", async () => {
    mockInvoke({ has_api_key: false, read_settings: {} });
    render(<SettingsAgents />);

    expect((await screen.findAllByRole("button", { name: "Add Key" })).length).toBeGreaterThan(0);
    expect(screen.queryByText("Unknown")).not.toBeInTheDocument();
  });

  it("offers no key action when the keychain cannot be read", async () => {
    mockInvoke({
      has_api_key: () => Promise.reject("the keychain is locked"),
      read_settings: {},
    });
    render(<SettingsAgents />);

    expect((await screen.findAllByText(/the keychain is locked/)).length).toBeGreaterThan(0);
    expect(screen.getAllByText("Unknown").length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: "Add Key" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update" })).not.toBeInTheDocument();
  });
});
