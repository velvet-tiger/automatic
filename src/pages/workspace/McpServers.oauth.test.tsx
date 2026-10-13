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
function routes(hasToken: () => boolean, tokenValid = true) {
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
      valid: hasToken() && tokenValid,
      revoked: false,
      message: null,
    }),
  };
}

function callsTo(command: string) {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === command);
}

async function editUrl(value = EDITED_URL) {
  const input = await screen.findByPlaceholderText("https://example.com/mcp");
  fireEvent.change(input, { target: { value } });
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

  it("offers neither sign-in nor revoke when the keychain cannot be read", async () => {
    mockInvoke({
      ...routes(() => false),
      has_mcp_oauth_token: () => Promise.reject("the keychain is locked"),
    });
    render(<McpServers initialServer="linear" />);

    expect(await screen.findByText(/the keychain is locked/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Authenticate" })).not.toBeInTheDocument();
    expect(screen.queryByText("Authenticated")).not.toBeInTheDocument();
    expect(callsTo("get_mcp_oauth_token_status")).toHaveLength(0);
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

  it("does not offer to sign in again while the URL is not saved", async () => {
    mockInvoke(routes(() => true, false));
    render(<McpServers initialServer="linear" />);

    const signInAgain = await screen.findByRole("button", { name: "Re-authenticate" });
    expect(signInAgain).toBeEnabled();

    await editUrl();

    expect(signInAgain).toBeDisabled();
    expect(screen.getByText("Save your changes before you sign in again.")).toBeInTheDocument();
    fireEvent.click(signInAgain);
    expect(callsTo("authorize_mcp_server")).toHaveLength(0);
  });

  it("does not check the token again after a save that keeps the URL", async () => {
    mockInvoke(routes(() => true));
    render(<McpServers initialServer="linear" />);
    await screen.findByText("Authenticated");
    await waitFor(() => expect(callsTo("get_mcp_oauth_token_status")).toHaveLength(1));

    await editUrl();
    await editUrl(SAVED_URL);
    fireEvent.click(await screen.findByRole("button", { name: "Save" }));

    await waitFor(() => expect(callsTo("save_mcp_server_config")).toHaveLength(1));
    await waitFor(() => expect(callsTo("has_mcp_oauth_token")).toHaveLength(2));
    expect(callsTo("get_mcp_oauth_token_status")).toHaveLength(1);
    expect(screen.getByText("Authenticated")).toBeInTheDocument();
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
    // Once on opening the server, and once after the save.
    expect(callsTo("has_mcp_oauth_token")).toHaveLength(2);
  });
});
