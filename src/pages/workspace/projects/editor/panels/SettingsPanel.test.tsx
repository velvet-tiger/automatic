import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SettingsPanel } from "./SettingsPanel";
import type { Project } from "../../types";

function project(overrides: Partial<Project> = {}): Project {
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
    ...overrides,
  } as Project;
}

function renderPanel(overrides: Partial<Parameters<typeof SettingsPanel>[0]> = {}) {
  const props = {
    project: project(),
    setProject: vi.fn(),
    setDirty: vi.fn(),
    onReadOnlyChange: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  render(<SettingsPanel {...props} />);
  return props;
}

describe("SettingsPanel read-only controls", () => {
  it("turns read-only mode on through its own callback, not a save", async () => {
    const props = renderPanel();
    fireEvent.click(screen.getByRole("switch", { name: "Read only on this computer" }));
    await waitFor(() => expect(props.onReadOnlyChange).toHaveBeenCalledWith(true));
    expect(props.setProject).not.toHaveBeenCalled();
    expect(props.setDirty).not.toHaveBeenCalled();
  });

  it("turns read-only mode off for a read-only project", async () => {
    const props = renderPanel({ project: project({ local_read_only: true }) });
    const toggle = screen.getByRole("switch", { name: "Read only on this computer" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(toggle);
    await waitFor(() => expect(props.onReadOnlyChange).toHaveBeenCalledWith(false));
  });

  it("saves the committed hint as a normal project edit", () => {
    const props = renderPanel();
    fireEvent.click(screen.getByRole("switch", { name: "Read only for new checkouts" }));
    expect(props.setProject).toHaveBeenCalledWith(expect.objectContaining({ read_only: true }));
    expect(props.setDirty).toHaveBeenCalledWith(true);
    expect(props.onReadOnlyChange).not.toHaveBeenCalled();
  });

  it("locks the settings that are saved to the folder while read only", () => {
    renderPanel({ project: project({ local_read_only: true, read_only: true }) });
    expect(
      (screen.getByRole("switch", { name: "Read only for new checkouts" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(
      (screen.getByRole("switch", { name: "Manage .gitignore for this project" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect((screen.getByRole("button", { name: "Silent" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows an inherited read-only state that is changed on the parent", () => {
    renderPanel({ project: project({ read_only_owner: "repo" }) });
    const toggle = screen.getByRole("switch", { name: "Read only on this computer" }) as HTMLButtonElement;
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    expect(toggle.disabled).toBe(true);
    expect(screen.getByText(/which is read only, so this project is too/)).toBeTruthy();
  });

  it("does not turn read only on for a project without a folder", () => {
    renderPanel({ project: project({ directory: "" }) });
    expect(
      (screen.getByRole("switch", { name: "Read only on this computer" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("waits for unsaved edits before turning read only on", () => {
    renderPanel({ hasUnsavedChanges: true });
    expect(
      (screen.getByRole("switch", { name: "Read only on this computer" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(screen.getByText(/Save your changes first/)).toBeTruthy();
  });
});
