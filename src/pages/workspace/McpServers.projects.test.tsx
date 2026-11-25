/**
 * The editor's Projects tab lists where a server is used and adds it to or
 * removes it from projects and profiles. A server installed from Discover
 * shows its catalogue card, matched on what it connects to.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../test/tauriMock";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import McpServers from "./McpServers";

const AIKIDO = { type: "stdio", command: "npx", args: ["-y", "@aikidosec/mcp@latest"], _author: { name: "Aikido Security" } };

const summaries = [
  { local_key: "app-key", id: "p-1", name: "app", directory: "/work/app" },
  { local_key: "web-key", id: "p-2", name: "web", directory: "/work/web" },
  { local_key: "api-key", id: "p-3", name: "api", directory: "/work/api" },
];

function routes(saved: Record<string, unknown>, overrides: Record<string, unknown> = {}) {
  return {
    list_mcp_server_configs: ["aikido-security"],
    read_mcp_server_config: JSON.stringify(saved),
    validate_stored_mcp_server_configs: {},
    validate_mcp_server_config: { findings: [], docker_fix_available: false, remote_equivalent: null },
    get_projects: [],
    get_project_summaries: summaries,
    list_mcp_server_scopes: {},
    list_accepted_mcp_scope_ids: { "app-key": "p-1" },
    list_mcp_server_connections: { "aikido-security": saved },
    get_recently_added_items: [],
    get_project_profiles: ["base", "security"],
    get_mcp_server_usage: {
      projects: [
        { name: "app", local_key: "app-key", directory: "/work/app", enabled: true, profiles: [], read_only: false },
        { name: "web", local_key: "web-key", directory: "/work/web", enabled: false, profiles: ["base"], read_only: false },
      ],
      profiles: ["base"],
    },
    check_mcp_command_available: { available: true, message: null },
    ...overrides,
  };
}

describe("McpServers Projects tab", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("lists projects and profiles, and only offers to remove a project's own entry", async () => {
    mockInvoke(routes(AIKIDO));
    render(<McpServers initialServer="aikido-security" />);

    expect(await screen.findByRole("tab", { name: "Projects", selected: true })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Remove from app" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove from web" })).not.toBeInTheDocument();
    expect(screen.getByText("From profile base")).toBeInTheDocument();
    expect(screen.getByText("Disabled")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove from profile base" })).toBeInTheDocument();
  });

  it("adds the server to a project and a profile that do not use it yet", async () => {
    mockInvoke(routes(AIKIDO, { attach_mcp_server_to_project: null, add_mcp_server_to_profile: null }));
    render(<McpServers initialServer="aikido-security" />);

    const projectPicker = await screen.findByLabelText("Project to add this server to");
    await within(projectPicker).findByRole("option", { name: "api" });
    expect(within(projectPicker).queryByRole("option", { name: "app" })).not.toBeInTheDocument();
    fireEvent.change(projectPicker, { target: { value: "api-key" } });
    fireEvent.click(within(projectPicker.parentElement!).getByRole("button", { name: /Add/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("attach_mcp_server_to_project", {
        projectName: "api-key",
        serverName: "aikido-security",
      }),
    );

    const profilePicker = screen.getByLabelText("Profile to add this server to");
    expect(within(profilePicker).queryByRole("option", { name: "base" })).not.toBeInTheDocument();
    fireEvent.change(profilePicker, { target: { value: "security" } });
    fireEvent.click(within(profilePicker.parentElement!).getByRole("button", { name: /Add/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("add_mcp_server_to_profile", {
        profileName: "security",
        serverName: "aikido-security",
      }),
    );
  });

  it("removes the server from a project and a profile after confirmation", async () => {
    mockInvoke(routes(AIKIDO, { detach_mcp_server_from_project: null, remove_mcp_server_from_profile: null }));
    render(<McpServers initialServer="aikido-security" />);

    fireEvent.click(await screen.findByRole("button", { name: "Remove from app" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("detach_mcp_server_from_project", {
        projectName: "app-key",
        serverName: "aikido-security",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Remove from profile base" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("remove_mcp_server_from_profile", {
        profileName: "base",
        serverName: "aikido-security",
      }),
    );
  });

  it("offers a project server only to its own checkout and never to a profile", async () => {
    mockInvoke(
      routes(AIKIDO, {
        list_mcp_server_scopes: { "aikido-security": "p-1" },
        get_mcp_server_usage: { projects: [], profiles: [] },
      }),
    );
    render(<McpServers initialServer="aikido-security" />);

    const projectPicker = await screen.findByLabelText("Project to add this server to");
    await within(projectPicker).findByRole("option", { name: "app" });
    expect(within(projectPicker).queryByRole("option", { name: "web" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Profile to add this server to")).not.toBeInTheDocument();
    expect(screen.getByText(/cannot go in a profile/)).toBeInTheDocument();
  });
});

describe("McpServers Discover card", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("shows the catalogue entry and links back to Discover", async () => {
    mockInvoke(routes(AIKIDO));
    const onNavigate = vi.fn();
    render(<McpServers initialServer="aikido-security" onNavigateToDiscoverMcp={onNavigate} />);

    expect(await screen.findByText("By Aikido Security")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /View in Discover/ }));
    expect(onNavigate).toHaveBeenCalledWith("aikido-security");
  });

  it("does not show the catalogue card for a server that only borrows the name", async () => {
    const lookalike = { ...AIKIDO, args: ["-y", "some-other-package"] };
    mockInvoke(routes(lookalike));
    render(<McpServers initialServer="aikido-security" onNavigateToDiscoverMcp={vi.fn()} />);

    expect(await screen.findByRole("tab", { name: "Projects" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /View in Discover/ })).not.toBeInTheDocument();
    expect(screen.queryByText("By Aikido Security")).not.toBeInTheDocument();
  });
});
