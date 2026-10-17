import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invokeMock, mockInvoke, resetInvokeMock, type InvokeArgs } from "../test/tauriMock";
import FirstRunWizard from "./FirstRunWizard";

vi.mock("@tauri-apps/api/path", () => ({
  homeDir: vi.fn().mockResolvedValue("/Users/me"),
}));

vi.mock("../lib/analytics", () => ({
  identifyOnboarding: vi.fn(),
  trackProjectCreated: vi.fn(),
}));

const report = {
  root: "/Users/me",
  visited: 40,
  unreadable: 0,
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
  ],
};

const baseRoutes = {
  read_settings: {},
  detect_installed_agents: "[]",
  has_api_key: false,
  detect_agent_global_configs: "[]",
  discover_projects: report,
};

/** Click through the first three steps to reach "Your projects". */
async function openProjectsStep(user: ReturnType<typeof userEvent.setup>) {
  render(<FirstRunWizard onComplete={() => {}} />);
  await user.click(await screen.findByText("Full-stack developer"));
  await user.click(screen.getByRole("button", { name: /Continue/ }));
  await user.click(await screen.findByText("AI-assisted"));
  await user.click(screen.getByRole("button", { name: /Continue/ }));
  await user.click(screen.getByRole("button", { name: /Continue/ }));
  await screen.findByText("Add the projects you already have");
  await waitFor(() =>
    expect((screen.getByLabelText("Folder to search") as HTMLInputElement).value).toBe("/Users/me"),
  );
}

describe("FirstRunWizard projects step", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("adds the ticked projects and moves on", async () => {
    const user = userEvent.setup();
    const calls: string[][] = [];
    mockInvoke({
      ...baseRoutes,
      add_discovered_projects: (args: InvokeArgs) => {
        const directories = (args?.directories as string[]) ?? [];
        calls.push(directories);
        return directories.map((directory) => ({
          directory,
          local_key: `key-${directory}`,
          name: directory.split("/").pop(),
          read_only: false,
          error: null,
        }));
      },
    });
    const events: string[] = [];
    const record = (e: Event) => events.push(e.type);
    window.addEventListener("project-added", record);
    window.addEventListener("project-updated", record);

    await openProjectsStep(user);
    // The search waits for the user.
    expect(invokeMock).not.toHaveBeenCalledWith("discover_projects", expect.anything());
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByLabelText("Add alpha"));

    await user.click(screen.getByRole("button", { name: /Add 1 project and continue/ }));

    await screen.findByText("Stay in the loop");
    expect(calls).toEqual([["/Users/me/code/alpha"]]);
    expect(events).toEqual(["project-added", "project-updated"]);

    // Back shows the result; the added folder cannot be added twice.
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByText("Added")).toBeInTheDocument();
    expect(screen.getByLabelText("Add alpha")).toBeDisabled();
    expect(screen.getByRole("button", { name: /^Continue/ })).toBeEnabled();

    window.removeEventListener("project-added", record);
    window.removeEventListener("project-updated", record);
  });

  it("stays on the step when a folder fails", async () => {
    const user = userEvent.setup();
    mockInvoke({
      ...baseRoutes,
      add_discovered_projects: [
        { directory: "/Users/me/code/alpha", local_key: null, name: null, read_only: false, error: "Cannot read folder" },
      ],
    });

    await openProjectsStep(user);
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByLabelText("Add alpha"));
    await user.click(screen.getByRole("button", { name: /Add 1 project and continue/ }));

    expect(await screen.findByText("Cannot read folder")).toBeInTheDocument();
    expect(screen.getByText("Add the projects you already have")).toBeInTheDocument();
    // The failed folder stays ticked so the user can retry.
    expect(screen.getByLabelText("Add alpha")).toBeChecked();
  });

  it("can be skipped without searching", async () => {
    const user = userEvent.setup();
    mockInvoke(baseRoutes);

    await openProjectsStep(user);
    await user.click(screen.getByRole("button", { name: "Skip for now" }));

    await screen.findByText("Stay in the loop");
    expect(invokeMock).not.toHaveBeenCalledWith("discover_projects", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("add_discovered_projects", expect.anything());
  });

  it("continues without adding when nothing is ticked", async () => {
    const user = userEvent.setup();
    mockInvoke(baseRoutes);

    await openProjectsStep(user);
    await user.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByText("Found 2 projects.");
    await user.click(screen.getByRole("button", { name: /^Continue/ }));

    await screen.findByText("Stay in the loop");
    expect(invokeMock).not.toHaveBeenCalledWith("add_discovered_projects", expect.anything());
  });

  it("stays on the step when the add call itself fails", async () => {
    const user = userEvent.setup();
    mockInvoke({
      ...baseRoutes,
      add_discovered_projects: () => {
        throw new Error("backend unavailable");
      },
    });

    await openProjectsStep(user);
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByLabelText("Add alpha"));
    await user.click(screen.getByRole("button", { name: /Add 1 project and continue/ }));

    expect(await screen.findByText(/Adding projects failed: .*backend unavailable/)).toBeInTheDocument();
    expect(screen.getByText("Add the projects you already have")).toBeInTheDocument();
  });

  it("announces a partial add once, stays, and retries only the failed folder", async () => {
    const user = userEvent.setup();
    const calls: string[][] = [];
    let attempt = 0;
    mockInvoke({
      ...baseRoutes,
      add_discovered_projects: (args: InvokeArgs) => {
        const directories = (args?.directories as string[]) ?? [];
        calls.push(directories);
        attempt += 1;
        return directories.map((directory) =>
          attempt === 1 && directory.endsWith("beta")
            ? { directory, local_key: null, name: null, read_only: false, error: "Disk full" }
            : { directory, local_key: `key-${directory}`, name: directory.split("/").pop(), read_only: false, error: null },
        );
      },
    });
    const events: string[] = [];
    const record = (e: Event) => events.push(e.type);
    window.addEventListener("project-added", record);

    await openProjectsStep(user);
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByLabelText("Select all"));
    await user.click(screen.getByRole("button", { name: /Add 2 projects and continue/ }));

    expect(await screen.findByText("Disk full")).toBeInTheDocument();
    expect(screen.getByText("Add the projects you already have")).toBeInTheDocument();
    expect(events).toEqual(["project-added"]);

    await user.click(screen.getByRole("button", { name: /Add 1 project and continue/ }));
    await screen.findByText("Stay in the loop");
    expect(calls).toEqual([
      ["/Users/me/code/alpha", "/Users/me/code/beta"],
      ["/Users/me/code/beta"],
    ]);
    window.removeEventListener("project-added", record);
  });

  it("ignores a scan that finishes after the step was skipped", async () => {
    const user = userEvent.setup();
    let finish: (value: unknown) => void = () => {};
    mockInvoke({ ...baseRoutes, discover_projects: () => new Promise((resolve) => (finish = resolve)) });

    await openProjectsStep(user);
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(screen.getByRole("button", { name: "Skip for now" }));
    await screen.findByText("Stay in the loop");
    finish(report);
    await user.click(screen.getByRole("button", { name: "Back" }));

    await screen.findByText("Add the projects you already have");
    expect(screen.queryByText(/Found/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Search" })).toBeEnabled();
  });
});
