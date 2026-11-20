import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { McpServersPanel } from "./McpServersPanel";
import type { ProfileLockMap, Project } from "../../types";

const noLocks: ProfileLockMap = {
  skills: {},
  mcp_servers: {},
  providers: {},
  agents: {},
  user_agents: {},
  user_commands: {},
  hooks: {},
  rules: {},
};

function project(overrides: Partial<Project> = {}): Project {
  return {
    name: "copy",
    id: "p-1",
    description: "",
    directory: "/tmp/copy",
    skills: [],
    mcp_servers: ["mine", "db"],
    providers: [],
    agents: [],
    created_at: "",
    updated_at: "",
    pending_project_mcp_servers: ["db"],
    ...overrides,
  } as Project;
}

function renderPanel(projectKey: string | null = "key") {
  const addItem = vi.fn().mockResolvedValue(true);
  render(
    <McpServersPanel
      project={project()}
      availableAgents={[]}
      availableMcpServers={["mine"]}
      profileLocks={noLocks}
      addItem={addItem}
      removeItem={vi.fn()}
      isMcpServerEnabled={() => true}
      toggleMcpServerEnabled={vi.fn()}
      aiMcpSuggestions={[]}
      aiMcpLoading={false}
      handleSuggestMcpServers={vi.fn()}
      removeRecommendation={vi.fn()}
      mcpServerScopes={{ mine: "p-1", db: "p-1" }}
      projectKey={projectKey}
      onMcpServersChanged={vi.fn()}
    />,
  );
  return { addItem };
}

describe("McpServersPanel project servers", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({ create_project_mcp_server: null });
  });

  it("marks accepted project servers only", () => {
    renderPanel();
    // Two project servers are attached, but "db" is not accepted here.
    expect(screen.getAllByTitle("Only this project can see this server")).toHaveLength(1);
  });

  it("creates a project server for this checkout by its key", async () => {
    const { addItem } = renderPanel();
    fireEvent.click(screen.getByRole("button", { name: /New project server/ }));
    fireEvent.change(screen.getByPlaceholderText(/Name, for example/), { target: { value: "local-db" } });
    fireEvent.change(screen.getByPlaceholderText(/Command, for example/), { target: { value: "npx -y db" } });
    fireEvent.click(screen.getByRole("button", { name: "Create and add" }));
    await waitFor(() => expect(addItem).toHaveBeenCalledWith("mcp_servers", "local-db"));
    expect(invokeMock).toHaveBeenCalledWith(
      "create_project_mcp_server",
      expect.objectContaining({ projectName: "key", name: "local-db" }),
    );
  });

  it("offers no new project server before the checkout has a key", () => {
    renderPanel(null);
    expect(screen.queryByRole("button", { name: /New project server/ })).not.toBeInTheDocument();
  });
});
