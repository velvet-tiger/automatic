/**
 * Characterization tests for Projects.tsx — pin the current list/editor
 * behavior that the Phase 2D carve-out must preserve.
 *
 * Each test mocks ONLY the commands needed for the behavior it exercises.
 * Unmocked commands resolve to `undefined` (see tauriMock.ts).
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../test/tauriMock";
import { renderProjects } from "../../test/renderProjects";
import { screen, waitFor, act, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// Minimal Project shape (only fields the overview/editor read at render time)
function emptyProject(name: string) {
  return {
    name,
    description: "",
    directory: "/tmp/" + name,
    skills: [],
    mcp_servers: [],
    disabled_mcp_servers: [],
    providers: [],
    agents: ["claude"],
    created_at: "2025-01-01T00:00:00Z",
    updated_at: "2025-01-01T00:00:00Z",
    file_rules: {},
    instruction_mode: "per-agent",
    custom_rules: [],
    tools: [],
    custom_agents: [],
    user_agents: [],
    custom_commands: [],
    user_commands: [],
    hooks: [],
    profiles: [],
    profile_contributions: {},
    group_profile_contributions: {},
    custom_skills: [],
    mode: "normal",
    directory_missing: false,
  };
}

/** A `get_project_summaries` row whose local_key is `key-<name>`. */
function summary(name: string) {
  return { name, local_key: `key-${name}`, id: `id-${name}`, directory: "/tmp/" + name };
}

/** Commands take a local_key; map it back to the display name for fixtures. */
function nameFromIdentifier(identifier: string | undefined): string {
  if (!identifier) return "x";
  return identifier.startsWith("key-") ? identifier.slice(4) : identifier;
}

function baselineRoutes(overrides: Record<string, unknown> = {}) {
  return {
    // List + details
    get_project_summaries: [],
    read_project: (args: any) =>
      JSON.stringify({ ...emptyProject(nameFromIdentifier(args?.name)), local_key: args?.name ?? "x" }),
    // Per-project drift + problems
    check_project_drift: JSON.stringify({ drifted: false, files: [] }),
    check_project_problems: JSON.stringify({ problems: [] }),
    // Settings / agents / inventory
    read_settings: { default_agents: [], default_agent_options: {} },
    list_agents: [],
    get_skills: [],
    list_mcp_server_configs: [],
    get_subagents: [],
    get_user_commands: [],
    get_hooks: [],
    get_instructions: [],
    get_rules: [],
    get_templates: [],
    list_groups: [],
    groups_for_project: [],
    list_tools_with_detection: [],
    agent_features_enabled: false,
    check_installed_editors: [],
    get_plugin_locked_resources: { skills: [], rules: [] },
    get_project_profiles: [],
    get_projects_referencing_profile: [],
    get_groups_referencing_profile: [],
    // Profile (no auth)
    read_profile: null,
    // Editor secondary loads (defaults: empty)
    get_project_memories: {},
    get_project_docs: "",
    get_project_activity: JSON.stringify([]),
    get_project_activity_paged: JSON.stringify([]),
    get_project_activity_count: 0,
    get_project_file_info: JSON.stringify([]),
    autodetect_project_dependencies: (args: any) =>
      JSON.stringify(emptyProject(nameFromIdentifier(args?.name))),
    evaluate_project_recommendations: [],
    list_recommendations_by_source: [],
    get_ai_recommendations_timestamp: null,
    // Mutations
    delete_project: undefined,
    save_project: undefined,
    sync_project: undefined,
    ...overrides,
  };
}

describe("Projects — characterization", () => {
  beforeEach(() => {
    resetInvokeMock();
    localStorage.clear();
  });

  it("B1: list renders project names from get_project_summaries", async () => {
    mockInvoke(baselineRoutes({ get_project_summaries: [summary("alpha"), summary("beta")] }));
    renderProjects();
    expect(await screen.findByText("alpha")).toBeInTheDocument();
    expect(await screen.findByText("beta")).toBeInTheDocument();
  });

  it("B2: clicking a project card opens the editor (calls read_project and shows project title h1)", async () => {
    mockInvoke(baselineRoutes({ get_project_summaries: [summary("alpha")] }));
    renderProjects();
    const card = await screen.findByRole("button", { name: /alpha/i });
    await userEvent.click(card);
    // Editor title (h1 with project name) appears
    await waitFor(() => {
      expect(
        screen.getByRole("heading", { level: 1, name: "alpha" }),
      ).toBeInTheDocument();
    });
    // The back button labelled "Projects" is the editor chrome
    expect(screen.getByTitle("Back to all projects")).toBeInTheDocument();
    // read_project was invoked with alpha's local_key, not its name
    const calls = invokeMock.mock.calls.map((c) => [c[0], c[1]]);
    expect(calls.some(([cmd, args]) => cmd === "read_project" && (args as any)?.name === "key-alpha")).toBe(true);
  });

  it("B3: back button returns to the list", async () => {
    mockInvoke(baselineRoutes({ get_project_summaries: [summary("alpha")] }));
    renderProjects();
    const card = await screen.findByRole("button", { name: /alpha/i });
    await userEvent.click(card);
    await screen.findByRole("heading", { level: 1, name: "alpha" });
    await userEvent.click(screen.getByTitle("Back to all projects"));
    await waitFor(() => {
      // Editor title gone; "Add Project" button (overview-only) is back
      expect(screen.queryByRole("heading", { level: 1, name: "alpha" })).toBeNull();
      expect(screen.getByRole("button", { name: /Add Project/i })).toBeInTheDocument();
    });
  });

  it("B4: clicking Add Project opens the wizard at step 1", async () => {
    mockInvoke(baselineRoutes({ get_project_summaries: [] }));
    renderProjects();
    // Empty-state CTA labelled "Create Project"
    const cta = await screen.findByRole("button", { name: /Create Project/i });
    await userEvent.click(cta);
    expect(
      await screen.findByRole("heading", { name: /Where is this project\?/i }),
    ).toBeInTheDocument();
  });

  it("B5: create-project window event triggers the wizard", async () => {
    mockInvoke(baselineRoutes({ get_project_summaries: [summary("alpha")] }));
    renderProjects();
    await screen.findByText("alpha");
    await act(async () => {
      window.dispatchEvent(new CustomEvent("create-project"));
    });
    expect(
      await screen.findByRole("heading", { name: /Where is this project\?/i }),
    ).toBeInTheDocument();
  });

  it("B6: project-removed event clears the selection when the open project is removed", async () => {
    mockInvoke(baselineRoutes({ get_project_summaries: [summary("alpha")] }));
    renderProjects();
    const card = await screen.findByRole("button", { name: /alpha/i });
    await userEvent.click(card);
    await screen.findByRole("heading", { level: 1, name: "alpha" });
    await act(async () => {
      window.dispatchEvent(
        new CustomEvent("project-removed", { detail: { name: "alpha", local_key: "key-alpha" } }),
      );
    });
    await waitFor(() => {
      expect(screen.queryByRole("heading", { level: 1, name: "alpha" })).toBeNull();
    });
  });

  it("B7: delete (handleRemove) calls delete_project and clears selection", async () => {
    let deleteCalled = false;
    let projectsList = [summary("alpha")];
    mockInvoke(
      baselineRoutes({
        get_project_summaries: () => [...projectsList],
        delete_project: (args: any) => {
          if ((args as any)?.name === "key-alpha") {
            projectsList = [];
            deleteCalled = true;
          }
          return undefined;
        },
      }),
    );
    renderProjects();
    const card = await screen.findByRole("button", { name: /alpha/i });
    await userEvent.click(card);
    await screen.findByRole("heading", { level: 1, name: "alpha" });
    // The header has a "Remove project" button (aria-label="Remove project")
    const removeBtn = screen.getByRole("button", { name: /Remove project/i });
    await userEvent.click(removeBtn);
    await waitFor(() => {
      expect(deleteCalled).toBe(true);
    });
    // Selection cleared → list visible
    await waitFor(() => {
      expect(screen.queryByRole("heading", { level: 1, name: "alpha" })).toBeNull();
    });
  });

  it("B8: two projects with one name show a folder hint; a unique name shows as before", async () => {
    mockInvoke(
      baselineRoutes({
        get_project_summaries: [
          { name: "website", local_key: "key-w1", id: "id-w1", directory: "/work/consultmed/website" },
          { name: "website", local_key: "key-w2", id: "id-w2", directory: "/work/_active/website" },
          summary("alpha"),
        ],
      }),
    );
    renderProjects();
    expect(await screen.findByText("· consultmed/website")).toBeInTheDocument();
    expect(screen.getByText("· _active/website")).toBeInTheDocument();
    const alpha = screen.getByText("alpha");
    expect(alpha.textContent).toBe("alpha");
  });

  it("B9: the wizard keeps a name another project uses, says so, and leaves Continue enabled", async () => {
    mockInvoke(
      baselineRoutes({
        get_project_summaries: [
          { name: "website", local_key: "key-w1", id: "id-w1", directory: "/work/consultmed/website" },
        ],
      }),
    );
    renderProjects();
    await userEvent.click(await screen.findByRole("button", { name: /Add Project/i }));
    await screen.findByRole("heading", { name: /Where is this project\?/i });
    fireEvent.change(screen.getByPlaceholderText("/path/to/your/project"), {
      target: { value: "/elsewhere/website" },
    });
    const nameField = await screen.findByLabelText("Project name");
    await waitFor(() => expect(nameField).toHaveValue("website"));
    expect(
      await screen.findByText(
        "Another project is also called \u201cwebsite\u201d (…/consultmed/website). You can keep this name.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Continue/i })).toBeEnabled();

    // A free name clears the note; an invalid one still disables Continue.
    fireEvent.change(nameField, { target: { value: "website-new" } });
    await waitFor(() => expect(screen.queryByText(/also called/)).toBeNull());
    expect(screen.getByRole("button", { name: /Continue/i })).toBeEnabled();
    fireEvent.change(nameField, { target: { value: "" } });
    expect(screen.getByRole("button", { name: /Continue/i })).toBeDisabled();
  });

  // Behaviors NOT covered automatically (gaps for the manual GUI checklist):
  // - B5 alt: initialCreateWithTemplate prop → template-seeded wizard (needs templates loaded first; covered manually).
  // - save→list-refresh (full save flow involves many side-effects; covered manually).
  // - drift modal opening + InstructionConflictModal flows.
  // The Phase 2D split MUST also re-verify all of the above by hand per Appendix D.
  it.todo("save refreshes overview drift/details (manual verification)");
  it.todo("create-from-template seeds wizard (manual verification)");
  it.todo("DriftDiffModal + InstructionConflictModal flows (manual verification)");
});

// Suppress unused warning for fireEvent (kept in import list for future tests)
void fireEvent;
