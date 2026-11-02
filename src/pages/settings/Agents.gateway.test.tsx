/**
 * The gateway token lives in the secret store. The Agents page sends a new
 * token there, never through settings, and never shows a stored one.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { invokeMock, mockInvoke, resetInvokeMock } from "../../test/tauriMock";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsAgents from "./Agents";

const invoked = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command);

const settings = (cf_token_stored: boolean) => ({
  agent_gateways: {
    anthropic: { account_id: "account", gateway_id: "gateway", ...(cf_token_stored ? { cf_token_stored } : {}) },
  },
});

async function openGateway(user: ReturnType<typeof userEvent.setup>) {
  const [toggle] = await screen.findAllByRole("button", { name: /Cloudflare AI Gateway/ });
  await user.click(toggle);
}

describe("Agents gateway token", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("saves a new token to the secret store and leaves it out of settings", async () => {
    const user = userEvent.setup();
    mockInvoke({
      has_api_key: true,
      read_settings: settings(false),
      list_agent_models: [],
      write_settings: undefined,
      save_gateway_token: undefined,
    });
    render(<SettingsAgents />);
    await openGateway(user);

    await user.type(screen.getByPlaceholderText("CF AIG Token (optional)"), " cf-new-token ");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(invoked("save_gateway_token")).toHaveLength(1));
    expect(invoked("save_gateway_token")[0][1]).toEqual({ provider: "anthropic", token: "cf-new-token" });
    expect(JSON.stringify(invoked("write_settings"))).not.toContain("cf-new-token");
  });

  it("keeps the stored token when the field is left empty", async () => {
    const user = userEvent.setup();
    mockInvoke({
      has_api_key: true,
      read_settings: settings(true),
      list_agent_models: [],
      write_settings: undefined,
    });
    render(<SettingsAgents />);
    await openGateway(user);

    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(invoked("write_settings")).toHaveLength(1));
    expect(JSON.stringify(invoked("write_settings"))).toContain("cf_token_stored");
    expect(invoked("save_gateway_token")).toHaveLength(0);
  });

  it("shows that a token is saved and can remove it", async () => {
    const user = userEvent.setup();
    mockInvoke({
      has_api_key: true,
      read_settings: settings(true),
      list_agent_models: [],
      delete_gateway_token: undefined,
    });
    render(<SettingsAgents />);
    await openGateway(user);

    const field = screen.getByPlaceholderText(/Token saved/);
    expect(field).toHaveValue("");
    expect(field).toHaveAttribute("type", "password");

    await user.click(screen.getByRole("button", { name: "Remove token" }));

    await waitFor(() => expect(screen.getByPlaceholderText("CF AIG Token (optional)")).toBeInTheDocument());
    expect(invoked("delete_gateway_token")[0][1]).toEqual({ provider: "anthropic" });
  });
});
