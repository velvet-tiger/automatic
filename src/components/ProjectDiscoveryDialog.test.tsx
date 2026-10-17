import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invokeMock, mockInvoke, resetInvokeMock, type InvokeArgs } from "../test/tauriMock";
import { ProjectDiscoveryDialog } from "./ProjectDiscoveryDialog";

vi.mock("@tauri-apps/api/path", () => ({
  homeDir: vi.fn().mockResolvedValue("/Users/me"),
}));

const report = {
  root: "/Users/me",
  visited: 120,
  unreadable: 2,
  truncated: false,
  projects: [
    {
      directory: "/Users/me/code/alpha",
      name: "alpha",
      relative_path: "code/alpha",
      agents: [{ id: "claude", label: "Claude Code" }],
      has_agents_md: false,
      status: { kind: "Available" },
    },
    {
      directory: "/Users/me/code/beta",
      name: "beta",
      relative_path: "code/beta",
      agents: [],
      has_agents_md: true,
      status: { kind: "Available" },
    },
    {
      directory: "/Users/me/code/old",
      name: "old",
      relative_path: "code/old",
      agents: [{ id: "cursor", label: "Cursor" }],
      has_agents_md: false,
      status: { kind: "RegisteredHere", name: "old", local_key: "k-old" },
    },
  ],
};

describe("ProjectDiscoveryDialog", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("searches the home folder and adds the ticked projects", async () => {
    const user = userEvent.setup();
    const added: { directories: string[] }[] = [];
    mockInvoke({
      discover_projects: report,
      add_discovered_projects: (args: InvokeArgs) => {
        const directories = (args?.directories as string[]) ?? [];
        added.push({ directories });
        return directories.map((directory) => ({ directory, local_key: `key-${directory}`, error: null }));
      },
    });
    const onAdded = vi.fn();

    render(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={onAdded} />);

    const folder = screen.getByLabelText("Folder to search") as HTMLInputElement;
    await waitFor(() => expect(folder.value).toBe("/Users/me"));
    expect(invokeMock).not.toHaveBeenCalledWith("discover_projects", expect.anything());

    await user.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByText("Found 3 projects.");
    expect(invokeMock).toHaveBeenCalledWith("discover_projects", { root: "/Users/me" });

    // New projects start ticked; a registered one cannot be ticked.
    expect(screen.getByLabelText("Add old")).toBeDisabled();
    expect(screen.getByText("Already added as old")).toBeInTheDocument();
    await user.click(screen.getByLabelText("Add beta"));

    await user.click(screen.getByRole("button", { name: "Add 1 project" }));

    await screen.findByText("Added");
    expect(added).toEqual([{ directories: ["/Users/me/code/alpha"] }]);
    expect(onAdded).toHaveBeenCalledWith([{ name: "alpha", localKey: "key-/Users/me/code/alpha" }]);
  });

  it("shows a per-folder error and keeps the folder addable", async () => {
    const user = userEvent.setup();
    mockInvoke({
      discover_projects: { ...report, projects: [report.projects[0]] },
      add_discovered_projects: [
        { directory: "/Users/me/code/alpha", local_key: null, error: "Cannot read folder" },
      ],
    });
    const onAdded = vi.fn();

    render(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={onAdded} />);
    await waitFor(() =>
      expect((screen.getByLabelText("Folder to search") as HTMLInputElement).value).toBe("/Users/me"),
    );
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByRole("button", { name: "Add 1 project" }));

    await screen.findByText("Cannot read folder");
    expect(onAdded).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Add alpha")).not.toBeDisabled();
  });
});
