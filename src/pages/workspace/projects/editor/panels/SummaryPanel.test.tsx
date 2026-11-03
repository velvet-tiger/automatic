import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SummaryPanel } from "./SummaryPanel";
import { emptyProject } from "../../helpers";
import type { ComponentProps } from "react";
import type { DriftedFile } from "../../types";

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

describe("SummaryPanel", () => {
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

  it("shows a placeholder before a memories load has started", () => {
    renderPanel({ memories: null, loadingMemories: false });
    expect(screen.getByRole("status", { name: "Loading" })).toBeInTheDocument();
    expect(screen.queryByText("Unavailable")).toBeNull();
  });

  it("shows unavailable when memories fail to load", () => {
    renderPanel({ memories: null, memoriesLoadFailed: true });
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
  });

  it("shows an error when activity fails to load", () => {
    renderPanel({ activityLoadFailed: true });
    expect(screen.getAllByText("Could not load activity.").length).toBeGreaterThan(0);
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
});
