/**
 * The library page limits a server to one checkout by its local_key, and
 * copies a project server through the backend so the copy keeps the stored
 * scope. A checkout that has not accepted its id's project servers cannot be
 * picked.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../test/tauriMock";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import McpServers from "./McpServers";

const summaries = [
  { local_key: "real-key", id: "p-1", name: "app", directory: "/work/app" },
  { local_key: "copy-key", id: "p-1", name: "app", directory: "/tmp/app" },
  { local_key: "lookalike-key", id: "p-1", name: "lookalike", directory: "/tmp/lookalike" },
  { local_key: "other-key", id: "p-2", name: "other", directory: "/work/other" },
];

function routes(server: string, saved: Record<string, unknown>) {
  return {
    list_mcp_server_configs: [server, "db"],
    read_mcp_server_config: JSON.stringify(saved),
    validate_stored_mcp_server_configs: {},
    validate_mcp_server_config: { findings: [], docker_fix_available: false, remote_equivalent: null },
    get_projects: [],
    get_project_summaries: summaries,
    list_mcp_server_scopes: { db: "p-1" },
    list_accepted_mcp_scope_ids: { "real-key": "p-1" },
    list_mcp_server_connections: {},
    get_mcp_server_usage: { projects: [], profiles: [] },
    get_project_profiles: [],
    get_recently_added_items: [],
    has_mcp_oauth_token: false,
    get_mcp_oauth_token_status: { has_token: false, valid: false, revoked: false, message: null },
    set_mcp_server_scope: null,
    duplicate_project_mcp_server: null,
  };
}

describe("McpServers project scope", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("limits by checkout key and blocks a checkout that has not accepted its id", async () => {
    mockInvoke(routes("shared", { command: "npx" }));
    render(<McpServers initialServer="shared" />);

    const picker = await screen.findByLabelText("Project to limit this server to");
    const copy = await within(picker).findByRole("option", { name: "app (/tmp/app): accept its project servers first" });
    expect(copy).toBeDisabled();
    expect(within(picker).getByRole("option", { name: "app (/work/app)" })).not.toBeDisabled();
    expect(within(picker).getByRole("option", { name: "other" })).not.toBeDisabled();
    expect(within(picker).getByRole("option", { name: "lookalike: accept its project servers first" })).toBeDisabled();

    fireEvent.change(picker, { target: { value: "real-key" } });
    fireEvent.click(screen.getByRole("button", { name: "Limit" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_mcp_server_scope", { name: "shared", projectName: "real-key" }),
    );
  });

  it("blocks a checkout whose id another checkout accepted, though the id owns no server", async () => {
    mockInvoke({ ...routes("shared", { command: "npx" }), list_mcp_server_scopes: {} });
    render(<McpServers initialServer="shared" />);
    expect(
      await screen.findByRole("option", { name: "app (/tmp/app): accept its project servers first" }),
    ).toBeDisabled();
  });

  it("copies a project server through the backend, which keeps its scope", async () => {
    mockInvoke(routes("db", { command: "npx", _project: "p-1" }));
    render(<McpServers initialServer="db" />);

    // Named by the checkout that accepted the id, never by a copy.
    expect(await screen.findByText("app", { selector: "span" })).toBeInTheDocument();
    expect(screen.queryByText("lookalike", { selector: "span" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Duplicate/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "duplicate_project_mcp_server",
        expect.objectContaining({ source: "db", name: "db-copy" }),
      ),
    );
    const call = invokeMock.mock.calls.find(([cmd]) => cmd === "duplicate_project_mcp_server");
    expect(JSON.parse((call?.[1] as { data: string }).data)).not.toHaveProperty("_project");
  });
});
