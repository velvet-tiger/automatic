import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { invokeMock, mockInvoke } from "../../../../../test/tauriMock";
import userEvent from "@testing-library/user-event";
import { SummaryPanel } from "./SummaryPanel";
import { clearGitActivityCache } from "../useProjectGitActivity";
import { emptyProject } from "../../helpers";
import type { ComponentProps } from "react";
import type { DriftedFile, GitActivity } from "../../types";

type PanelProps = ComponentProps<typeof SummaryPanel>;

function renderPanel(overrides: Partial<PanelProps> = {}) {
  const selectTab = vi.fn();
  const project = {
    ...emptyProject("kelpie"),
    directory: "/work/kelpie",
    agents: ["claude"],
    skills: ["one", "two"],
  };
  render(
    <SummaryPanel
      project={project}
      projectKey="kelpie-key"
      isCreating={false}
      memories={{}}
      projectFiles={[]}
      linkDocEntries={[]}
      recsDisplayCount={0}
      activityEntries={[]}
      loadingActivity={false}
      activityLoadFailed={false}
      loadingMemories={false}
      memoriesLoadFailed={false}
      driftReport={{ drifted: false, agents: [] }}
      driftCheckFailed={false}
      availableAgents={[]}
      selectTab={selectTab}
      updateField={vi.fn()}
      {...overrides}
    />,
  );
  return selectTab;
}

const hoursAgo = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();

const REPO: GitActivity = {
  repo: { branch: "main", uncommitted_files: 3, upstream: "origin/main", ahead: 2, behind: 0, has_commits: true },
  commits: [
    {
      hash: "aaaaaaaa1",
      timestamp: hoursAgo(2),
      author: "Sam",
      subject: "fix(auth): refresh token on 401",
      insertions: 10,
      deletions: 4,
      touches_agent_config: false,
    },
    {
      hash: "bbbbbbbb2",
      timestamp: hoursAgo(30),
      author: "Chris",
      subject: "chore: update CLAUDE.md rules",
      insertions: 2,
      deletions: 1,
      touches_agent_config: true,
    },
  ],
  truncated: false,
  history_days: 98,
  line_stats: true,
};

describe("SummaryPanel", () => {
  beforeEach(() => {
    clearGitActivityCache();
    invokeMock.mockReset();
    mockInvoke({ get_project_git_activity: null });
  });

  it("opens the tab for a bar segment", async () => {
    const selectTab = renderPanel();
    const [segment] = screen.getAllByRole("button", { name: "View 2 skills" });
    await userEvent.click(segment);
    expect(selectTab).toHaveBeenCalledWith("skills");
  });

  it("opens the add tab for an empty configuration type", async () => {
    const selectTab = renderPanel();
    await userEvent.click(screen.getByRole("button", { name: "Add Hooks" }));
    expect(selectTab).toHaveBeenCalledWith("hooks");
  });

  it.each([
    ["No instruction files", "project_file"],
    ["No docs links", "docs_links"],
    ["View memory", "memory"],
    ["View sync log", "activity"],
  ])("opens the tab from the footer link %s", async (name, tab) => {
    const selectTab = renderPanel();
    await userEvent.click(screen.getByRole("button", { name }));
    expect(selectTab).toHaveBeenCalledWith(tab);
  });

  it("shows placeholders while loading, never a zero", () => {
    renderPanel({ loadingActivity: true, loadingMemories: true, memories: null });
    expect(screen.getAllByRole("status", { name: "Loading" }).length).toBeGreaterThan(0);
    expect(screen.queryByText("0")).toBeNull();
    expect(screen.queryByText("Not synced yet")).toBeNull();
  });

  it("shows a placeholder before a memories load has started", async () => {
    renderPanel({ memories: null, loadingMemories: false });
    // Git placeholders clear once the folder reads as not a repository.
    await waitFor(() => expect(screen.getAllByRole("status", { name: "Loading" })).toHaveLength(1));
    expect(screen.queryByText("Unavailable")).toBeNull();
  });

  it("shows unavailable when memories fail to load", () => {
    renderPanel({ memories: null, memoriesLoadFailed: true });
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
  });

  it("shows an error when activity fails to load", () => {
    renderPanel({ activityLoadFailed: true });
    expect(screen.getByText("Last sync unavailable")).toBeInTheDocument();
    expect(screen.queryByText("Not synced yet")).toBeNull();
  });

  it("wraps the last sync line within two lines instead of cutting it short", () => {
    renderPanel();
    const line = screen.getByText("Not synced yet");
    expect(line).toHaveClass("line-clamp-2");
    expect(line).not.toHaveClass("truncate");
  });

  it("names project-level drift in the Agents card", () => {
    renderPanel({
      driftReport: { drifted: true, agents: [], gitignore: { path: ".gitignore" } as DriftedFile },
    });
    expect(screen.getByText("Out of sync")).toBeInTheDocument();
    expect(screen.getByText("Project files also differ: .gitignore.")).toBeInTheDocument();
  });

  it("shows not set up for agents with no directory", () => {
    renderPanel({ project: { ...emptyProject("kelpie"), agents: ["claude"] } });
    expect(screen.getAllByText("Not set up")).toHaveLength(2);
  });

  it("leaves the git cards out for a folder that is not a repository", async () => {
    renderPanel();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_project_git_activity", { project: "kelpie-key" }));
    await waitFor(() => expect(screen.queryByText("Recent commits")).toBeNull());
    expect(screen.queryByText("Commits, 30 days")).toBeNull();
    expect(screen.queryByText("Commits")).toBeNull();
  });

  it("shows commits, the repository line and the recent list for a repository", async () => {
    mockInvoke({ get_project_git_activity: REPO });
    renderPanel();
    expect(await screen.findByText("fix(auth): refresh token on 401")).toBeInTheDocument();
    expect(screen.getByText("main")).toBeInTheDocument();
    expect(screen.getByText("3 uncommitted files")).toBeInTheDocument();
    expect(screen.getByText("2 ahead, 0 behind")).toBeInTheDocument();
    expect(screen.getByText("Agent config")).toBeInTheDocument();
    expect(screen.getByText("2 authors, +12 −5 lines")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /Commit calendar.*2 commits/ })).toBeInTheDocument();
  });

  it("says no upstream when the branch has none", async () => {
    mockInvoke({
      get_project_git_activity: { ...REPO, repo: { ...REPO.repo, upstream: null, ahead: null, behind: null } },
    });
    renderPanel();
    expect(await screen.findByText("No upstream")).toBeInTheDocument();
  });

  it("says the upstream is gone when it was deleted", async () => {
    mockInvoke({ get_project_git_activity: { ...REPO, repo: { ...REPO.repo, ahead: null, behind: null } } });
    renderPanel();
    expect(await screen.findByText("Upstream origin/main is gone")).toBeInTheDocument();
  });

  it("leaves line counts out when git cannot count them", async () => {
    mockInvoke({ get_project_git_activity: { ...REPO, line_stats: false } });
    renderPanel();
    expect(await screen.findByText("2 authors")).toBeInTheDocument();
    expect(screen.queryByText(/lines/)).toBeNull();
  });

  it("asks for the project by its local key, not its name", async () => {
    renderPanel();
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    const projects = invokeMock.mock.calls
      .filter(([cmd]) => cmd === "get_project_git_activity")
      .map(([, args]) => (args as { project: string }).project);
    expect(new Set(projects)).toEqual(new Set(["kelpie-key"]));
  });

  it("shows the reason when git history cannot be read", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "get_project_git_activity" ? Promise.reject("git log failed in /work/kelpie: boom") : Promise.resolve(undefined),
    );
    renderPanel();
    expect(await screen.findByText(/Couldn't read git history\. git log failed/)).toBeInTheDocument();
    expect(screen.queryByText("Recent commits")).toBeNull();
  });

  it("shows git's safe.directory command when git refuses the folder's owner", async () => {
    const command = "git config --global --add safe.directory '%(prefix)///wsl.localhost/Ubuntu/home/me/app'";
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "get_project_git_activity"
        ? Promise.reject(`Git will not read this repository, because its folder belongs to a different user. If you trust it, run this command once, then try again:\n${command}`)
        : Promise.resolve(undefined),
    );
    renderPanel();
    expect(await screen.findByText(command)).toBeInTheDocument();
    expect(screen.getByText(/Couldn't read git history\. Git will not read/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy command" })).toBeInTheDocument();
  });

  it("does not read git history while a project is being created", () => {
    renderPanel({ isCreating: true });
    expect(invokeMock).not.toHaveBeenCalledWith("get_project_git_activity", expect.anything());
  });
});
