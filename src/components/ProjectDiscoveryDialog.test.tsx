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
      check_error: null,
    },
    {
      directory: "/Users/me/code/beta",
      name: "beta",
      relative_path: "code/beta",
      agents: [],
      has_agents_md: true,
      status: { kind: "Available" },
      check_error: null,
    },
    {
      directory: "/Users/me/code/old",
      name: "old",
      relative_path: "code/old",
      agents: [{ id: "cursor", label: "Cursor" }],
      has_agents_md: false,
      status: { kind: "RegisteredHere", name: "old", local_key: "k-old" },
      check_error: null,
    },
    {
      directory: "/Users/me/code/stray",
      name: "stray",
      relative_path: "code/stray",
      agents: [],
      has_agents_md: true,
      status: { kind: "OrphanConfig", name: "stray" },
      check_error: null,
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
        return directories.map((directory) => ({
          directory,
          local_key: `key-${directory}`,
          name: directory.endsWith("stray") ? "stray-config" : directory.split("/").pop(),
          read_only: directory.endsWith("stray"),
          error: null,
        }));
      },
    });
    const onAdded = vi.fn();

    render(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={onAdded} />);

    const folder = screen.getByLabelText("Folder to search") as HTMLInputElement;
    await waitFor(() => expect(folder.value).toBe("/Users/me"));
    expect(invokeMock).not.toHaveBeenCalledWith("discover_projects", expect.anything());

    await user.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByText("Found 4 projects.");
    expect(invokeMock).toHaveBeenCalledWith("discover_projects", { root: "/Users/me" });

    // Nothing starts ticked. A registered folder cannot be ticked; a folder
    // with an unregistered config can, and is noted as an import.
    expect(screen.getByLabelText("Add alpha")).not.toBeChecked();
    expect(screen.getByLabelText("Add old")).toBeDisabled();
    expect(screen.getByLabelText("Add stray")).toBeEnabled();
    expect(screen.getByText("Already added as old")).toBeInTheDocument();
    expect(screen.getByText(/It will be imported as/)).toHaveTextContent("It will be imported as stray.");
    await user.click(screen.getByLabelText("Add alpha"));
    await user.click(screen.getByLabelText("Add stray"));

    await user.click(screen.getByRole("button", { name: "Add 2 projects" }));

    await screen.findByText("Added as read only");
    expect(screen.getByText("Added")).toBeInTheDocument();
    expect(added).toEqual([{ directories: ["/Users/me/code/alpha", "/Users/me/code/stray"] }]);
    expect(onAdded).toHaveBeenCalledWith([
      { name: "alpha", localKey: "key-/Users/me/code/alpha" },
      { name: "stray-config", localKey: "key-/Users/me/code/stray" },
    ]);
  });

  it("shows a per-folder error and keeps the folder addable", async () => {
    const user = userEvent.setup();
    mockInvoke({
      discover_projects: { ...report, projects: [report.projects[0]] },
      add_discovered_projects: [
        { directory: "/Users/me/code/alpha", local_key: null, name: null, read_only: false, error: "Cannot read folder" },
      ],
    });
    const onAdded = vi.fn();

    render(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={onAdded} />);
    await waitFor(() =>
      expect((screen.getByLabelText("Folder to search") as HTMLInputElement).value).toBe("/Users/me"),
    );
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByLabelText("Add alpha"));
    await user.click(screen.getByRole("button", { name: "Add 1 project" }));

    await screen.findByText("Cannot read folder");
    expect(onAdded).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Add alpha")).not.toBeDisabled();
    // A failed folder stays ticked so the user can retry.
    expect(screen.getByLabelText("Add alpha")).toBeChecked();
  });

  it("selects every addable folder with Select all", async () => {
    const user = userEvent.setup();
    mockInvoke({ discover_projects: report });

    render(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={() => {}} />);
    await waitFor(() =>
      expect((screen.getByLabelText("Folder to search") as HTMLInputElement).value).toBe("/Users/me"),
    );
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByLabelText("Select all"));

    expect(screen.getByLabelText("Add alpha")).toBeChecked();
    expect(screen.getByLabelText("Add beta")).toBeChecked();
    expect(screen.getByLabelText("Add stray")).toBeChecked();
    expect(screen.getByLabelText("Add old")).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Add 3 projects" })).toBeEnabled();
  });

  it("drops a scan that finishes after the dialog closed", async () => {
    const user = userEvent.setup();
    let finish: (value: unknown) => void = () => {};
    mockInvoke({ discover_projects: () => new Promise((resolve) => (finish = resolve)) });

    const { rerender } = render(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={() => {}} />);
    await waitFor(() =>
      expect((screen.getByLabelText("Folder to search") as HTMLInputElement).value).toBe("/Users/me"),
    );
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(screen.getByRole("button", { name: "Close" }));
    rerender(<ProjectDiscoveryDialog isOpen={false} onClose={() => {}} onAdded={() => {}} />);
    finish(report);
    rerender(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={() => {}} />);

    await waitFor(() => expect(screen.getByRole("button", { name: "Search" })).toBeEnabled());
    expect(screen.queryByText(/Found/)).not.toBeInTheDocument();
  });

  it("shows a folder that could not be checked and does not let it be ticked", async () => {
    const user = userEvent.setup();
    mockInvoke({
      discover_projects: {
        ...report,
        projects: [{ ...report.projects[0], status: null, check_error: "registry entry is unreadable" }],
      },
    });

    render(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={() => {}} />);
    await waitFor(() =>
      expect((screen.getByLabelText("Folder to search") as HTMLInputElement).value).toBe("/Users/me"),
    );
    await user.click(screen.getByRole("button", { name: "Search" }));

    await screen.findByText("Could not check this folder: registry entry is unreadable");
    expect(screen.getByLabelText("Add alpha")).toBeDisabled();
    expect(screen.queryByLabelText("Select all")).not.toBeInTheDocument();
  });

  it("stays locked while onAdded runs and shows the problem it reports", async () => {
    const user = userEvent.setup();
    mockInvoke({
      discover_projects: { ...report, projects: [report.projects[0]] },
      add_discovered_projects: [
        { directory: "/Users/me/code/alpha", local_key: "k-alpha", name: "alpha", read_only: false, error: null },
      ],
    });
    let release: (value: string | null) => void = () => {};
    const onAdded = vi.fn(() => new Promise<string | null>((resolve) => (release = resolve)));

    render(<ProjectDiscoveryDialog isOpen onClose={() => {}} onAdded={onAdded} />);
    await waitFor(() =>
      expect((screen.getByLabelText("Folder to search") as HTMLInputElement).value).toBe("/Users/me"),
    );
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByLabelText("Add alpha"));
    await user.click(screen.getByRole("button", { name: "Add 1 project" }));

    await waitFor(() => expect(onAdded).toHaveBeenCalled());
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();

    release("Could not join the group");
    expect(await screen.findByText("Could not join the group")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeEnabled();
  });
});
