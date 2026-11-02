import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { ContextsPanel } from "./ContextsPanel";
import type { Project } from "../../types";

const LIBRARY = [
  { slug: "own", display_name: "Own", description: "", location: "local", sources: [], created_at: "", updated_at: "" },
  { slug: "shared", display_name: "Shared", description: "", location: "local", sources: [], created_at: "", updated_at: "" },
  { slug: "extra", display_name: "Extra", description: "", location: "cloud", context_id: "ctx_1", created_at: "", updated_at: "" },
];

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
    contexts: ["own", "shared"],
    group_context_contributions: { team: ["shared"] },
  } as Project;
}

function renderPanel(overrides: Partial<Parameters<typeof ContextsPanel>[0]> = {}) {
  const props = {
    project: project(),
    setProject: vi.fn(),
    dirty: false,
    setDirty: vi.fn(),
    isCreating: false,
    selectedKey: "app",
    reloadProject: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  render(<ContextsPanel {...props} />);
  return props;
}

describe("ContextsPanel", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({ list_contexts: LIBRARY, decide_project_context: null, attach_context: true });
  });

  it("marks group-provided contexts and only lets you remove the project's own", async () => {
    renderPanel();
    await screen.findByText("From group: team");
    expect(screen.getAllByTitle("Remove from this project")).toHaveLength(1);
    expect(screen.getByLabelText("Remove Own")).toBeInTheDocument();
  });

  it("attaches through the backend when the project is saved and clean", async () => {
    const props = renderPanel();
    fireEvent.click(await screen.findByText("Attach a context"));
    fireEvent.click(await screen.findByText("Extra"));
    fireEvent.click(screen.getByText("Attach"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("attach_context", { target: { type: "project", name: "app" }, slug: "extra" });
      expect(props.reloadProject).toHaveBeenCalledWith("app");
    });
  });

  it("only edits state while the project has unsaved changes", async () => {
    const props = renderPanel({ dirty: true });
    fireEvent.click(await screen.findByText("Attach a context"));
    fireEvent.click(await screen.findByText("Extra"));
    fireEvent.click(screen.getByText("Attach"));
    await waitFor(() => {
      expect(props.setProject).toHaveBeenCalledWith(expect.objectContaining({ contexts: ["own", "shared", "extra"] }));
    });
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "attach_context")).toBe(false);
  });

  it("offers a way to create a context when none are attached", async () => {
    const onNavigateToContexts = vi.fn();
    renderPanel({ project: { ...project(), contexts: [], group_context_contributions: {} }, onNavigateToContexts });
    fireEvent.click(await screen.findByText("Create one in the Library"));
    expect(onNavigateToContexts).toHaveBeenCalled();
  });

  it("records the click at once while the editor holds unsaved edits", async () => {
    const props = renderPanel({ dirty: true });
    fireEvent.click(await screen.findByRole("button", { name: "Remove Own" }));
    await waitFor(() => expect(props.setProject).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith(
      "decide_project_context",
      expect.objectContaining({ projectName: "app", slug: "own", confirmed: false }),
    );
  });

  it("records nothing in the create wizard", async () => {
    const props = renderPanel({ isCreating: true, selectedKey: null });
    fireEvent.click(await screen.findByRole("button", { name: "Remove Own" }));
    await waitFor(() => expect(props.setProject).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("decide_project_context", expect.anything());
  });

  it("marks pending and ignored slugs and attaches from the row", async () => {
    const props = renderPanel({
      project: {
        ...project(),
        contexts: ["own", "extra"],
        pending_contexts: ["own", "extra"],
        local_ignored_contexts: ["extra"],
        group_context_contributions: {},
      },
    });
    expect(await screen.findByText("Pending")).toBeInTheDocument();
    expect(screen.getByText("Ignored")).toBeInTheDocument();
    const buttons = screen.getAllByRole("button", { name: "Attach" });
    expect(buttons).toHaveLength(2);

    fireEvent.click(buttons[1]);
    await waitFor(() => expect(props.reloadProject).toHaveBeenCalledWith("app"));
    expect(invokeMock).toHaveBeenCalledWith(
      "attach_context",
      expect.objectContaining({ target: { type: "project", name: "app" }, slug: "extra" }),
    );
  });

  it("keeps unsaved edits when attaching from a row", async () => {
    const props = renderPanel({
      dirty: true,
      project: { ...project(), contexts: ["own"], pending_contexts: ["own"], group_context_contributions: {} },
    });
    fireEvent.click(await screen.findByRole("button", { name: "Attach" }));
    await waitFor(() => expect(props.setProject).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith(
      "decide_project_context",
      expect.objectContaining({ slug: "own", confirmed: true }),
    );
    expect(props.reloadProject).not.toHaveBeenCalled();
    expect(vi.mocked(props.setProject).mock.calls[0][0].pending_contexts).toEqual([]);
  });

  it("reports an unsaved attach, so a discard can take the confirmation back", async () => {
    const onUnsavedAttach = vi.fn();
    renderPanel({ dirty: true, onUnsavedAttach });
    fireEvent.click(await screen.findByRole("button", { name: /Attach a context/ }));
    fireEvent.click(await screen.findByText("Extra"));
    fireEvent.click(await screen.findByRole("button", { name: /^Attach$/ }));
    await waitFor(() => expect(onUnsavedAttach).toHaveBeenCalledWith("extra"));
  });
});
