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
    custom_skills: [],
    mode: "normal",
    directory_missing: false,
  };
}

function baselineRoutes(overrides: Record<string, unknown> = {}) {
  return {
    // List + details
    get_projects: [],
    read_project: (args: any) => JSON.stringify(emptyProject(args?.name ?? "x")),
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
    get_project_templates: [],
    list_groups: [],
    groups_for_project: [],
    list_tools_with_detection: [],
    agent_features_enabled: false,
    check_installed_editors: [],
    get_plugin_locked_resources: { skills: [], rules: [] },
    // Profile (no auth)
    read_profile: null,
    // Editor secondary loads (defaults: empty)
    get_project_memories: {},
    get_project_context: "",
    read_project_context_raw: "",
    get_project_docs: "",
    get_project_activity: JSON.stringify([]),
    get_project_activity_paged: JSON.stringify([]),
    get_project_activity_count: 0,
    get_project_file_info: JSON.stringify([]),
    autodetect_project_dependencies: (args: any) =>
      JSON.stringify(emptyProject(args?.name ?? "x")),
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

  it("B1: list renders project names from get_projects", async () => {
    mockInvoke(baselineRoutes({ get_projects: ["alpha", "beta"] }));
    renderProjects();
    expect(await screen.findByText("alpha")).toBeInTheDocument();
    expect(await screen.findByText("beta")).toBeInTheDocument();
  });

  it("B2: clicking a project card opens the editor (calls read_project and shows project title h1)", async () => {
    mockInvoke(baselineRoutes({ get_projects: ["alpha"] }));
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
    // read_project was invoked for "alpha"
    const calls = invokeMock.mock.calls.map((c) => [c[0], c[1]]);
    expect(calls.some(([cmd, args]) => cmd === "read_project" && (args as any)?.name === "alpha")).toBe(true);
  });

  it("B3: back button returns to the list", async () => {
    mockInvoke(baselineRoutes({ get_projects: ["alpha"] }));
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
    mockInvoke(baselineRoutes({ get_projects: [] }));
    renderProjects();
    // Empty-state CTA labelled "Create Project"
    const cta = await screen.findByRole("button", { name: /Create Project/i });
    await userEvent.click(cta);
    expect(
      await screen.findByRole("heading", { name: /Where is this project\?/i }),
    ).toBeInTheDocument();
  });

  it("B5: create-project window event triggers the wizard", async () => {
    mockInvoke(baselineRoutes({ get_projects: ["alpha"] }));
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
    mockInvoke(baselineRoutes({ get_projects: ["alpha"] }));
    renderProjects();
    const card = await screen.findByRole("button", { name: /alpha/i });
    await userEvent.click(card);
    await screen.findByRole("heading", { level: 1, name: "alpha" });
    await act(async () => {
      window.dispatchEvent(
        new CustomEvent("project-removed", { detail: { name: "alpha" } }),
      );
    });
    await waitFor(() => {
      expect(screen.queryByRole("heading", { level: 1, name: "alpha" })).toBeNull();
    });
  });

  it("B7: delete (handleRemove) calls delete_project and clears selection", async () => {
    let deleteCalled = false;
    let projectsList = ["alpha"];
    mockInvoke(
      baselineRoutes({
        get_projects: () => [...projectsList],
        delete_project: (args: any) => {
          if ((args as any)?.name === "alpha") {
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
