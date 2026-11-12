/**
 * Adding a catalogue server whose URL differs per account must ask for the
 * account name, and must never save the URL with its placeholder.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import DiscoverMcp from "./DiscoverMcp";

const TEMPLATE = "https://<account-name>.app.spacelift.io/mcp";

const SPACELIFT = {
  slug: "spacelift",
  name: "io.spacelift/mcp",
  title: "Spacelift",
  description: "Manage Spacelift stacks.",
  provider: "Spacelift",
  classification: "official",
  repository_url: null,
  remote: { transport: "http", url: TEMPLATE },
  local: null,
  auth: { method: "oauth", env_vars: [] },
};

function routes(installed: string[]) {
  return {
    search_discover_mcp: JSON.stringify([SPACELIFT]),
    list_mcp_server_configs: installed,
    get_skills: [],
    validate_mcp_server_config: { findings: [], docker_fix_available: false, remote_equivalent: null },
    save_mcp_server_config: null,
  };
}

function saved(): Array<{ name: string; config: Record<string, unknown> }> {
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "save_mcp_server_config")
    .map(([, args]) => {
      const { name, data } = args as { name: string; data: string };
      return { name, config: JSON.parse(data) };
    });
}

describe("DiscoverMcp URL template", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("asks for the account name before adding", async () => {
    mockInvoke(routes([]));
    render(<DiscoverMcp initialSlug="spacelift" />);

    const add = await screen.findByRole("button", { name: /Add to MCP Servers/ });
    expect(add).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Account name"), { target: { value: "evil.com" } });
    expect(add).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Account name"), { target: { value: "acme" } });
    expect(add).toBeEnabled();
    fireEvent.click(add);

    await waitFor(() => expect(saved()).toHaveLength(1));
    expect(saved()[0]).toEqual({
      name: "spacelift",
      config: {
        type: "http",
        url: "https://acme.app.spacelift.io/mcp",
        _url_template: TEMPLATE,
        _author: { name: "Spacelift" },
      },
    });
  });

  it("asks for a fresh account name for another copy", async () => {
    mockInvoke(routes(["spacelift"]));
    render(<DiscoverMcp initialSlug="spacelift" />);

    fireEvent.click(await screen.findByRole("button", { name: /Add another copy/ }));
    const field = screen.getByLabelText("Account name") as HTMLInputElement;
    expect(field.value).toBe("");
    const add = screen.getByRole("button", { name: "Add" });
    expect(add).toBeDisabled();

    fireEvent.change(field, { target: { value: "second" } });
    fireEvent.keyDown(field, { key: "Enter" });

    await waitFor(() => expect(saved()).toHaveLength(1));
    expect(saved()[0].name).toBe("spacelift-2");
    expect(saved()[0].config.url).toBe("https://second.app.spacelift.io/mcp");
  });
});
