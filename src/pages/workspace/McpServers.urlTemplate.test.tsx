/**
 * A catalogue server whose URL differs per account, such as Spacelift, must
 * stay repairable in the library editor although its other settings are
 * locked. The user types only the account name.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import McpServers from "./McpServers";

const TEMPLATE = "https://<account-name>.app.spacelift.io/mcp";

function routes(saved: Record<string, unknown>) {
  return {
    list_mcp_server_configs: ["spacelift"],
    read_mcp_server_config: JSON.stringify(saved),
    validate_stored_mcp_server_configs: {},
    validate_mcp_server_config: { findings: [], docker_fix_available: false, remote_equivalent: null },
    get_projects: [],
    get_recently_added_items: [],
    has_mcp_oauth_token: false,
    get_mcp_oauth_token_status: { has_token: false, valid: false, revoked: false, message: null },
  };
}

function savedConfigs(): Record<string, unknown>[] {
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "save_mcp_server_config")
    .map(([, args]) => JSON.parse((args as { data: string }).data));
}

describe("McpServers URL template", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("repairs a server saved with the raw template", async () => {
    mockInvoke(routes({ type: "http", url: TEMPLATE, _author: { name: "Spacelift" } }));
    render(<McpServers initialServer="spacelift" />);

    const field = await screen.findByLabelText("Account name");
    expect(screen.getByText("Enter your account name.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Authenticate" })).not.toBeInTheDocument();

    fireEvent.change(field, { target: { value: "acme" } });
    expect(screen.getByText("https://acme.app.spacelift.io/mcp")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "Save" }));

    await waitFor(() => expect(savedConfigs()).toHaveLength(1));
    expect(savedConfigs()[0]).toMatchObject({
      url: "https://acme.app.spacelift.io/mcp",
      _url_template: TEMPLATE,
      _author: { name: "Spacelift" },
    });
  });

  it("shows the saved account name and keeps the template on save", async () => {
    mockInvoke(
      routes({
        type: "http",
        url: "https://acme.app.spacelift.io/mcp",
        _url_template: TEMPLATE,
        _author: { name: "Spacelift" },
      }),
    );
    render(<McpServers initialServer="spacelift" />);

    const field = (await screen.findByLabelText("Account name")) as HTMLInputElement;
    expect(field.value).toBe("acme");
    expect(await screen.findByRole("button", { name: "Authenticate" })).toBeEnabled();

    fireEvent.change(field, { target: { value: "other" } });
    fireEvent.click(await screen.findByRole("button", { name: "Save" }));

    await waitFor(() => expect(savedConfigs()).toHaveLength(1));
    expect(savedConfigs()[0]).toMatchObject({
      url: "https://other.app.spacelift.io/mcp",
      _url_template: TEMPLATE,
    });
  });

  it("refuses a value that would move the server to another host", async () => {
    mockInvoke(
      routes({
        type: "http",
        url: "https://acme.app.spacelift.io/mcp",
        _url_template: TEMPLATE,
        _author: { name: "Spacelift" },
      }),
    );
    render(<McpServers initialServer="spacelift" />);

    fireEvent.change(await screen.findByLabelText("Account name"), { target: { value: "evil.com/x" } });

    expect(screen.getByText(/can contain only letters, digits and hyphens/)).toBeInTheDocument();
    expect(screen.queryByText(/evil\.com/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Authenticate" })).not.toBeInTheDocument();
  });

  it("keeps the URL locked when the stored template is not in the catalogue", async () => {
    mockInvoke(
      routes({
        type: "http",
        url: "https://acme.app.spacelift.io.evil.example/mcp",
        _url_template: "https://<account-name>.app.spacelift.io.evil.example/mcp",
        _author: { name: "Spacelift" },
      }),
    );
    render(<McpServers initialServer="spacelift" />);

    const url = (await screen.findByPlaceholderText("https://example.com/mcp")) as HTMLInputElement;
    expect(url.readOnly).toBe(true);
    expect(screen.queryByLabelText("Account name")).not.toBeInTheDocument();
  });

  it("does not save the raw template over a working URL", async () => {
    mockInvoke(
      routes({
        type: "http",
        url: "https://acme.app.spacelift.io/mcp",
        _url_template: TEMPLATE,
        _author: { name: "Spacelift" },
      }),
    );
    render(<McpServers initialServer="spacelift" />);

    fireEvent.change(await screen.findByLabelText("Account name"), { target: { value: "acme." } });
    const save = await screen.findByRole("button", { name: "Save" });
    expect(save).toBeDisabled();
    fireEvent.click(save);
    expect(savedConfigs()).toHaveLength(0);
  });
});
