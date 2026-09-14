import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock } from "../../test/tauriMock";
import { render, screen, waitFor } from "@testing-library/react";
import Profiles from "./Profiles";

describe("Profiles (smoke)", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({
      get_project_profiles: [],
      get_recently_added_items: [],
      list_agents: [],
      get_subagents: [],
      get_user_commands: [],
      get_hooks: [],
      get_skills: [],
      list_mcp_server_configs: [],
      get_rules: [],
      get_projects: [],
    });
  });

  it("renders the empty state without throwing", async () => {
    render(<Profiles />);
    await waitFor(() => {
      expect(screen.getByText(/No profiles yet/i)).toBeInTheDocument();
    });
  });

  it("lists profiles with their summary", async () => {
    mockInvoke({
      get_project_profiles: ["baseline"],
      read_project_profile: JSON.stringify({
        name: "baseline",
        description: "Shared baseline",
        skills: ["react"],
        mcp_servers: [],
        providers: [],
        agents: [],
        rules: ["automatic-process", "automatic-prose"],
      }),
      get_recently_added_items: [],
      list_agents: [],
      get_subagents: [],
      get_user_commands: [],
      get_hooks: [],
      get_skills: [],
      list_mcp_server_configs: [],
      get_rules: [],
      get_projects: [],
    });
    render(<Profiles />);
    await waitFor(() => {
      expect(screen.getByText("baseline")).toBeInTheDocument();
      expect(screen.getByText("1 skill · 2 rules")).toBeInTheDocument();
    });
  });
});
