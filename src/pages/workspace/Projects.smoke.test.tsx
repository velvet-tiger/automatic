import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock } from "../../test/tauriMock";
import { renderProjects } from "../../test/renderProjects";
import { screen, waitFor } from "@testing-library/react";

describe("Projects (smoke)", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({
      get_projects: [],
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
      list_tools_with_detection: [],
      agent_features_enabled: false,
      read_settings: {},
      check_installed_editors: [],
      get_plugin_locked_resources: { skills: [], rules: [] },
      read_profile: null,
    });
  });

  it("renders the overview without throwing", async () => {
    renderProjects();
    await waitFor(() => {
      expect(screen.getByText(/No projects yet/i)).toBeInTheDocument();
    });
  });
});
