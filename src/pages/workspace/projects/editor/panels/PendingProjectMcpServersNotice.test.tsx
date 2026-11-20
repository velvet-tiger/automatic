import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { PendingProjectMcpServersNotice } from "./PendingProjectMcpServersNotice";
import type { Project } from "../../types";

function project(overrides: Partial<Project> = {}): Project {
  return {
    name: "copy",
    id: "p-1",
    description: "",
    directory: "/tmp/copy",
    skills: [],
    mcp_servers: ["db"],
    providers: [],
    agents: [],
    created_at: "",
    updated_at: "",
    pending_project_mcp_servers: ["db"],
    ...overrides,
  } as Project;
}

const summaries = [
  { local_key: "key", id: "p-1", name: "copy", directory: "/tmp/copy" },
  { local_key: "real-key", id: "p-1", name: "real", directory: "/work/real" },
  { local_key: "other-key", id: "p-2", name: "other", directory: "/work/other" },
];

function renderNotice(overrides: { readOnly?: boolean; project?: Project } = {}) {
  const reloadProject = vi.fn().mockResolvedValue(undefined);
  const onSync = vi.fn();
  render(
    <PendingProjectMcpServersNotice
      project={overrides.project ?? project()}
      selectedKey="key"
      names={["db"]}
      readOnly={overrides.readOnly ?? false}
      reloadProject={reloadProject}
      onSync={onSync}
    />,
  );
  return { reloadProject, onSync };
}

describe("PendingProjectMcpServersNotice", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({
      get_project_summaries: summaries,
      confirm_project_mcp_servers: null,
      ignore_project_mcp_servers: null,
    });
  });

  it("names the servers and the other folders with the same id", async () => {
    renderNotice();
    expect(
      screen.getByText('This project id has MCP servers that this folder does not use yet: "db".'),
    ).toBeInTheDocument();
    expect(await screen.findByText("real: /work/real")).toBeInTheDocument();
    expect(screen.queryByText(/other: /)).not.toBeInTheDocument();
    expect(screen.queryByText(/copy: /)).not.toBeInTheDocument();
  });

  it("accepts the id it was shown, reloads, and syncs", async () => {
    const { reloadProject, onSync } = renderNotice();
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(onSync).toHaveBeenCalled());
    expect(reloadProject).toHaveBeenCalledWith("key");
    expect(invokeMock).toHaveBeenCalledWith(
      "confirm_project_mcp_servers",
      expect.objectContaining({ projectName: "key", projectId: "p-1" }),
    );
  });

  it("does not sync a read-only project after accepting", async () => {
    const { reloadProject, onSync } = renderNotice({ readOnly: true });
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(reloadProject).toHaveBeenCalledWith("key"));
    expect(onSync).not.toHaveBeenCalled();
  });

  it("ignores for this checkout without syncing", async () => {
    const { reloadProject, onSync } = renderNotice();
    fireEvent.click(screen.getByRole("button", { name: "Ignore" }));
    await waitFor(() => expect(reloadProject).toHaveBeenCalledWith("key"));
    expect(onSync).not.toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith(
      "ignore_project_mcp_servers",
      expect.objectContaining({ projectName: "key", projectId: "p-1" }),
    );
  });

  it("shows a refused accept", async () => {
    mockInvoke({
      get_project_summaries: summaries,
      confirm_project_mcp_servers: () => {
        throw "The id changed";
      },
    });
    const { onSync } = renderNotice();
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    expect(await screen.findByText(/Could not accept this project's MCP servers: The id changed/)).toBeInTheDocument();
    expect(onSync).not.toHaveBeenCalled();
  });
});
