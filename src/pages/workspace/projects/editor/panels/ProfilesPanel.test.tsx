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

  it("offers Attach and Ignore for a pending profile, and no Detach", async () => {
    const props = renderPanel({
      project: { ...project(), pending_profiles: ["own"] },
    });
    expect(screen.getByText("This project's shared settings include this profile. It is not turned on for this computer yet.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Detach own")).not.toBeInTheDocument();

    fireEvent.click(screen.getByText("Ignore"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("ignore_project_profile", { projectName: "app", profileName: "own" });
    });
    await waitFor(() => expect(props.reloadProject).toHaveBeenCalledWith("app"));
  });

  it("keeps Attach for an ignored profile", () => {
    renderPanel({
      project: { ...project(), pending_profiles: ["own"], local_ignored_profiles: ["own"] },
    });
    expect(screen.getByText("Ignored on this computer. Nothing from it applies here.")).toBeInTheDocument();
    expect(screen.getByText("Attach")).toBeInTheDocument();
    expect(screen.queryByText("Ignore")).not.toBeInTheDocument();
  });

  it("hides Attach on a read-only project and keeps Ignore", () => {
    renderPanel({ project: { ...project(), pending_profiles: ["own"] }, readOnly: true });
    expect(screen.queryByText("Attach")).not.toBeInTheDocument();
    expect(screen.getByText("Ignore")).toBeInTheDocument();
  });

  it("asks about a pending profile a group lists instead of showing the group", () => {
    renderPanel({ project: { ...project(), pending_profiles: ["shared"] } });
    expect(screen.queryByText("From group: team")).not.toBeInTheDocument();
    expect(screen.getByText("Attach")).toBeInTheDocument();
  });

  it("names a group this checkout has not joined", () => {
    renderPanel({
      project: { ...project(), pending_profiles: ["shared"] },
      unjoinedGroupProfiles: { shared: "team" },
    });
    expect(
      screen.getByText('From the group "team", which this checkout has not joined. Nothing from it applies here yet.'),
    ).toBeInTheDocument();
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
