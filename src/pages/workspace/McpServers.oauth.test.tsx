/**
 * The OAuth section of the MCP server editor must never use a URL that is
 * not saved yet. A token is issued for one URL, and the proxy sends it to the
 * saved one.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import McpServers from "./McpServers";

const SAVED_URL = "https://mcp.example.com/mcp";
const EDITED_URL = "https://other.example/mcp";

/** Routes for an editor showing one saved HTTP server named `linear`. */
function routes(hasToken: () => boolean) {
  return {
    list_mcp_server_configs: ["linear"],
    read_mcp_server_config: JSON.stringify({ type: "http", url: SAVED_URL }),
    validate_stored_mcp_server_configs: {},
    validate_mcp_server_config: { findings: [], docker_fix_available: false, remote_equivalent: null },
    get_projects: [],
    get_recently_added_items: [],
    has_mcp_oauth_token: () => hasToken(),
    get_mcp_oauth_token_status: () => ({
      has_token: hasToken(),
      valid: hasToken(),
      revoked: false,
      message: null,
    }),
  };
}

function callsTo(command: string) {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === command);
}

async function editUrl() {
  const input = await screen.findByPlaceholderText("https://example.com/mcp");
  fireEvent.change(input, { target: { value: EDITED_URL } });
}

describe("McpServers OAuth section", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("does not offer sign-in while the URL is not saved", async () => {
    mockInvoke(routes(() => false));
    render(<McpServers initialServer="linear" />);

    const signIn = await screen.findByRole("button", { name: "Authenticate" });
    expect(signIn).toBeEnabled();

    await editUrl();

    expect(signIn).toBeDisabled();
    expect(screen.getByText("Save your changes before you sign in.")).toBeInTheDocument();
    fireEvent.click(signIn);
    expect(callsTo("authorize_mcp_server")).toHaveLength(0);
  });

  it("does not send the stored token to a URL that is not saved", async () => {
    mockInvoke(routes(() => true));
    render(<McpServers initialServer="linear" />);

    await screen.findByText("Authenticated");
    await waitFor(() => expect(callsTo("get_mcp_oauth_token_status")).toHaveLength(1));

    await editUrl();
    // Give a wrongly started check time to show up.
    await new Promise((resolve) => setTimeout(resolve, 50));

    const checkedUrls = callsTo("get_mcp_oauth_token_status").map(([, args]) => args.mcpUrl);
    expect(checkedUrls).toEqual([SAVED_URL]);
  });

  it("asks again for the stored token after a save", async () => {
    // The backend deletes the token when a save changes the URL.
    let tokenStored = true;
    mockInvoke({
      ...routes(() => tokenStored),
      save_mcp_server_config: () => {
        tokenStored = false;
      },
    });
    render(<McpServers initialServer="linear" />);
    await screen.findByText("Authenticated");

    await editUrl();
    fireEvent.click(await screen.findByRole("button", { name: "Save" }));

    await waitFor(() => expect(screen.queryByText("Authenticated")).not.toBeInTheDocument());
    expect(await screen.findByRole("button", { name: "Authenticate" })).toBeEnabled();
    expect(screen.queryByText("Authentication expired")).not.toBeInTheDocument();
  });
});
