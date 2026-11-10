import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { WritableFoldersCard } from "./WritableFoldersCard";
import type { Project, WritableFoldersFindings } from "../../types";

vi.mock("@tauri-apps/api/path", () => ({ homeDir: vi.fn().mockResolvedValue("/Users/me") }));

function project(overrides: Partial<Project> = {}): Project {
  return {
    name: "app",
    description: "",
    directory: "/Users/me/app",
    skills: [],
    mcp_servers: [],
    providers: [],
    agents: ["claude"],
    created_at: "",
    updated_at: "",
    local_writable_folders: ["/Users/me/work/docs", "/Users/me/old/proto"],
    ...overrides,
  } as Project;
}

const findings: WritableFoldersFindings = {
  wanted: [
    { path: "/Users/me/work/docs", source: { kind: "extra" } },
    { path: "/Users/me/work/cli", source: { kind: "group_member", group: "Automatic", project: "automatic-cli" } },
  ],
  uncovered: [],
  missing: [{ path: "/Users/me/old/proto", source: { kind: "extra" } }],
  target: { kind: "local" },
  blocked_by: null,
  ask_for_user_settings: false,
  paste_json: "{}",
  unreadable: [],
  groups_switched_off: ["Clients"],
  last_sync_errors: [],
  refused: [],
  left_in_tracked_local: [],
};

describe("WritableFoldersCard", () => {
  beforeEach(() => {
    resetInvokeMock();
    vi.mocked(openDialog).mockReset();
    mockInvoke({
      read_project_writable_folders: findings,
      set_project_writable_folders: { target: { kind: "local" }, wanted: [], errors: [] },
    });
  });

  it("shows nothing for a project without Claude Code", () => {
    const { container } = render(
      <WritableFoldersCard project={project({ agents: ["codex"] })} projectKey="app" onChanged={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("shows folders by name, where they were saved, and the groups", async () => {
    const onNavigateToGroup = vi.fn();
    render(<WritableFoldersCard project={project()} projectKey="app" onChanged={vi.fn()} onNavigateToGroup={onNavigateToGroup} />);
    expect(await screen.findByText("Saved for this project only")).toBeInTheDocument();
    expect(screen.getByText("docs")).toBeInTheDocument();
    expect(await screen.findByText("~/work")).toBeInTheDocument();
    expect(screen.getByText("Not on this computer")).toBeInTheDocument();
    expect(screen.getByText(/automatic-cli/)).toBeInTheDocument();
    fireEvent.click(screen.getByText("Open the group"));
    expect(onNavigateToGroup).toHaveBeenCalledWith("Clients");
  });

  it("adds a picked folder and saves the whole list", async () => {
    vi.mocked(openDialog).mockResolvedValue("/Users/me/work/meta");
    const onChanged = vi.fn().mockResolvedValue(undefined);
    render(<WritableFoldersCard project={project()} projectKey="app" onChanged={onChanged} />);
    fireEvent.click(screen.getByText("Add folder…"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("set_project_writable_folders", {
      name: "app",
      folders: ["/Users/me/work/docs", "/Users/me/old/proto", "/Users/me/work/meta"],
    });
  });

  it("removes a folder", async () => {
    const onChanged = vi.fn().mockResolvedValue(undefined);
    render(<WritableFoldersCard project={project()} projectKey="app" onChanged={onChanged} />);
    fireEvent.click(screen.getByLabelText("Remove proto"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("set_project_writable_folders", { name: "app", folders: ["/Users/me/work/docs"] });
  });

  it("shows a refused folder as an error and keeps the list", async () => {
    vi.mocked(openDialog).mockResolvedValue("/Volumes/Shared");
    mockInvoke({
      read_project_writable_folders: findings,
      set_project_writable_folders: () => {
        throw "Claude Code may not edit '/Volumes/Shared', because it is outside your home folder.";
      },
    });
    const onChanged = vi.fn();
    render(<WritableFoldersCard project={project()} projectKey="app" onChanged={onChanged} />);
    fireEvent.click(screen.getByText("Add folder…"));
    expect(await screen.findByText(/outside your home folder/)).toBeInTheDocument();
    expect(onChanged).not.toHaveBeenCalled();
  });
});
