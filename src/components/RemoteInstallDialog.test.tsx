import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invokeMock, mockInvoke, resetInvokeMock, type InvokeArgs } from "../test/tauriMock";
import RemoteInstallDialog from "./RemoteInstallDialog";

const manifest = {
  name: "toolkit",
  version: "1.0.0",
  description: "Test",
  skills: { entries: [] },
  mcp_servers: [
    { name: "automatic", path: "a.json", description: "" },
    { name: "github", path: "g.json", description: "" },
  ],
  rules: [
    { name: "style", path: "s.md", description: "" },
    { name: "fresh", path: "f.md", description: "" },
  ],
  templates: [],
  commands: [],
  agents: [],
  collections: [{ path: "c.json", slug: "starter", name: "Starter" }],
};

const conflicts = [
  {
    resource_type: "mcp_server",
    resource_name: "automatic",
    existing_source: "built-in",
    existing_name: "automatic",
    blocked: true,
    reason: "'automatic' is built-in and cannot be replaced",
  },
  {
    resource_type: "rule",
    resource_name: "style",
    existing_source: "local",
    existing_name: "style",
    blocked: false,
    reason: "Replaces your own 'style'",
  },
];

describe("RemoteInstallDialog", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("checks conflicts after the fetch and sends the confirmed overrides", async () => {
    const user = userEvent.setup();
    const installs: InvokeArgs[] = [];
    mockInvoke({
      fetch_remote_source: JSON.stringify(manifest),
      check_source_conflicts: JSON.stringify(conflicts),
      install_remote_source: (args: InvokeArgs) => {
        installs.push(args);
        return JSON.stringify({ installed: { rules: ["style", "fresh"] }, skipped: [], warnings: [] });
      },
    });

    render(
      <RemoteInstallDialog isOpen repo="acme/toolkit" onClose={() => {}} onInstalled={vi.fn()} />,
    );

    await screen.findByText("Cannot install: 'automatic' is built-in and cannot be replaced");
    expect(invokeMock).toHaveBeenCalledWith("check_source_conflicts", {
      repo: "acme/toolkit",
      dir: null,
    });
    expect(screen.getByLabelText("Install automatic")).toBeDisabled();
    expect(screen.getByLabelText("Install style")).not.toBeChecked();
    expect(screen.getByLabelText("Install fresh")).toBeChecked();
    expect(screen.getByLabelText("Install starter")).toBeChecked();

    // Group clicks clear and refill only the conflict-free rules. They never
    // confirm the conflict.
    await user.click(screen.getByLabelText("Select all Rules"));
    expect(screen.getByLabelText("Install fresh")).not.toBeChecked();
    await user.click(screen.getByLabelText("Select all Rules"));
    expect(screen.getByLabelText("Install fresh")).toBeChecked();
    expect(screen.getByLabelText("Install style")).not.toBeChecked();

    await user.click(screen.getByLabelText("Install style"));
    await user.click(screen.getByRole("button", { name: /Install \(/ }));

    await screen.findByText(/Successfully installed/);
    expect(installs).toHaveLength(1);
    expect(installs[0]?.overrides).toEqual(["rule:style"]);
    const selected = JSON.parse(installs[0]?.selected as string);
    expect(selected.rules.sort()).toEqual(["fresh", "style"]);
    expect(selected.mcp_servers).toEqual(["github"]);
    expect(selected.collections).toEqual(["starter"]);
  });
});
