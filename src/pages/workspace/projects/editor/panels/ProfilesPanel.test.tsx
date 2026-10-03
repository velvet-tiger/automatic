import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { ProfilesPanel } from "./ProfilesPanel";
import type { Project } from "../../types";

function project(): Project {
  return {
    name: "app",
    description: "",
    directory: "/tmp/app",
    skills: [],
    mcp_servers: [],
    providers: [],
    agents: [],
    created_at: "",
    updated_at: "",
    profiles: ["own", "shared"],
    profile_contributions: {},
    group_profile_contributions: { team: ["shared"] },
  } as Project;
}

function renderPanel(overrides: Partial<Parameters<typeof ProfilesPanel>[0]> = {}) {
  const props = {
    project: project(),
    setProject: vi.fn(),
    setDirty: vi.fn(),
    isCreating: false,
    selectedKey: "app",
    availableProfiles: ["own", "shared", "extra"],
    reloadProject: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  render(<ProfilesPanel {...props} />);
  return props;
}

describe("ProfilesPanel", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({});
  });

  it("marks group-provided profiles and only lets you detach the project's own", () => {
    renderPanel();
    expect(screen.getByText("From group: team")).toBeInTheDocument();
    expect(screen.getAllByTitle("Detach profile")).toHaveLength(1);
    expect(screen.getByLabelText("Detach own")).toBeInTheDocument();
    expect(screen.queryByLabelText("Detach shared")).not.toBeInTheDocument();
  });

  it("opens the group from the badge", () => {
    const onNavigateToGroup = vi.fn();
    renderPanel({ onNavigateToGroup });
    fireEvent.click(screen.getByText("From group: team"));
    expect(onNavigateToGroup).toHaveBeenCalledWith("team");
  });

  it("detaches the project's own profile through the backend", async () => {
    const props = renderPanel();
    fireEvent.click(screen.getByLabelText("Detach own"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("detach_profile_from_project", { projectName: "app", profileName: "own" });
      expect(props.reloadProject).toHaveBeenCalledWith("app");
    });
  });
});
