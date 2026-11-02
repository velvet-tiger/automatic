import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { PendingProfilesNotice } from "./PendingProfilesNotice";
import type { Project } from "../../types";

function project(overrides: Partial<Project> = {}): Project {
  return {
    name: "app",
    description: "",
    directory: "/tmp/app",
    skills: ["react"],
    mcp_servers: ["GitHub"],
    providers: [],
    agents: [],
    created_at: "",
    updated_at: "",
    profiles: ["team"],
    pending_profiles: ["team"],
    ...overrides,
  } as Project;
}

describe("PendingProfilesNotice", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({
      read_project_profile: JSON.stringify({
        name: "team",
        skills: ["react", "vitest"],
        mcp_servers: ["github", "sentry"],
      }),
    });
  });

  it("lists only what the profile would add", async () => {
    render(<PendingProfilesNotice project={project()} selectedKey="app" reloadProject={vi.fn()} />);
    expect(screen.getByText('This repository asks for the profile "team".')).toBeInTheDocument();
    expect(
      await screen.findByText("Attaching it adds 1 skill: vitest · 1 MCP server: sentry."),
    ).toBeInTheDocument();
  });

  it("attaches through the backend and reloads", async () => {
    const reloadProject = vi.fn().mockResolvedValue(undefined);
    render(<PendingProfilesNotice project={project()} selectedKey="app" reloadProject={reloadProject} />);
    fireEvent.click(screen.getByText("Attach"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("attach_profile_to_project", {
        projectName: "app",
        profileName: "team",
      });
    });
    await waitFor(() => expect(reloadProject).toHaveBeenCalledWith("app"));
  });

  it("records Ignore through the backend", async () => {
    render(<PendingProfilesNotice project={project()} selectedKey="app" reloadProject={vi.fn()} />);
    fireEvent.click(screen.getByText("Ignore"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("ignore_project_profile", {
        projectName: "app",
        profileName: "team",
      });
    });
  });

  it("shows nothing for an ignored name", () => {
    render(
      <PendingProfilesNotice
        project={project({ local_ignored_profiles: ["team"] })}
        selectedKey="app"
        reloadProject={vi.fn()}
      />,
    );
    expect(screen.queryByText("Attach")).not.toBeInTheDocument();
  });
});
