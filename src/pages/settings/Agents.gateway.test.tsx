/**
 * The custom gateway form on the Claude card. It works with no Anthropic key,
 * shows the backend's error text, and replaces the Cloudflare section.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { invokeMock, mockInvoke, resetInvokeMock, type InvokeArgs } from "../../test/tauriMock";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import SettingsAgents from "./Agents";

const OFF = { configured: false, base_url: null, credential_kind: null };
const ON = { configured: true, base_url: "https://gateway.example.com", credential_kind: "bearer" };

function commandCalls(command: string) {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === command);
}

async function openForm() {
  fireEvent.click(await screen.findByRole("button", { name: /Custom gateway/ }));
}

describe("Agents settings custom gateway", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("is available with no Anthropic key and warns about the plain text token", async () => {
    mockInvoke({ has_api_key: false, read_settings: {}, get_custom_gateway_status: OFF });
    render(<SettingsAgents />);

    await openForm();

    expect(screen.getByLabelText("Gateway URL")).toBeInTheDocument();
    expect(screen.getByLabelText("Gateway token")).toHaveAttribute("type", "password");
    expect(screen.getByText(/in plain text\s+to ~\/\.claude\/settings\.json/)).toBeInTheDocument();
    expect(screen.getByText(/instead of your claude\.ai login/)).toBeInTheDocument();
    expect(screen.getByText(/Remote Control and voice dictation/)).toBeInTheDocument();
    expect(screen.getByText(/Claude desktop app has its own gateway settings/)).toBeInTheDocument();
  });

  it("saves through its own command and reloads the status", async () => {
    let status: typeof OFF | typeof ON = OFF;
    mockInvoke({
      has_api_key: false,
      read_settings: {},
      get_custom_gateway_status: () => status,
      save_custom_gateway: () => {
        status = ON;
      },
    });
    render(<SettingsAgents />);
    await openForm();

    const save = screen.getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Gateway URL"), {
      target: { value: "https://gateway.example.com" },
    });
    fireEvent.change(screen.getByLabelText("Credential type"), { target: { value: "api_key" } });
    fireEvent.change(screen.getByLabelText("Gateway token"), { target: { value: "token" } });
    fireEvent.click(save);

    await screen.findByText("Active");
    expect(commandCalls("save_custom_gateway")[0][1]).toEqual({
      baseUrl: "https://gateway.example.com",
      credentialKind: "api_key",
      token: "token",
    });
    expect(commandCalls("write_settings")).toHaveLength(0);
  });

  it("shows the backend's error and keeps the form open", async () => {
    mockInvoke({
      has_api_key: false,
      read_settings: {},
      get_custom_gateway_status: OFF,
      save_custom_gateway: () =>
        Promise.reject("ANTHROPIC_API_KEY is already set in /home/me/.claude/settings.json"),
    });
    render(<SettingsAgents />);
    await openForm();

    fireEvent.change(screen.getByLabelText("Gateway URL"), {
      target: { value: "https://gateway.example.com" },
    });
    fireEvent.change(screen.getByLabelText("Gateway token"), { target: { value: "token" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText(/ANTHROPIC_API_KEY is already set/)).toBeInTheDocument();
    expect(screen.getByLabelText("Gateway URL")).toBeInTheDocument();
  });

  it("lets a saved gateway keep its token when the field is blank", async () => {
    mockInvoke({
      has_api_key: false,
      read_settings: {},
      get_custom_gateway_status: ON,
      list_agent_models: [],
    });
    render(<SettingsAgents />);
    await openForm();

    const url = await screen.findByLabelText("Gateway URL");
    await waitFor(() => expect(url).toHaveValue("https://gateway.example.com"));
    expect(
      screen.getByPlaceholderText("Leave blank to keep the current token"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(commandCalls("save_custom_gateway")).toHaveLength(1));
    expect(commandCalls("save_custom_gateway")[0][1]).toMatchObject({ token: null });
  });

  it("says which values a clear left in place", async () => {
    let status: typeof OFF | typeof ON = ON;
    mockInvoke({
      has_api_key: false,
      read_settings: {},
      get_custom_gateway_status: () => status,
      list_agent_models: [],
      clear_custom_gateway: () => {
        status = OFF;
        return { left_in_claude_code: ["ANTHROPIC_AUTH_TOKEN"] };
      },
    });
    render(<SettingsAgents />);
    await openForm();

    fireEvent.click(await screen.findByRole("button", { name: "Clear" }));

    expect(await screen.findByText(/ANTHROPIC_AUTH_TOKEN changed outside Automatic/)).toBeInTheDocument();
  });

  it("offers Claude as the active agent with no Anthropic key", async () => {
    mockInvoke({
      has_api_key: false,
      read_settings: {},
      get_custom_gateway_status: ON,
      list_agent_models: [],
    });
    render(<SettingsAgents />);

    expect(await screen.findByText("Active agent")).toBeInTheDocument();
    const claude = await screen.findByRole("button", { name: /Claude.*Custom gateway/ });
    expect(claude).toBeEnabled();
  });

  it("hides the Cloudflare section while the gateway is active", async () => {
    const hasKey = (args: InvokeArgs) => args?.provider === "anthropic";
    mockInvoke({
      has_api_key: hasKey,
      read_settings: {},
      get_custom_gateway_status: OFF,
      list_agent_models: [],
    });
    const { unmount } = render(<SettingsAgents />);
    expect((await screen.findAllByText("Cloudflare AI Gateway")).length).toBeGreaterThan(0);
    unmount();

    mockInvoke({
      has_api_key: hasKey,
      read_settings: {},
      get_custom_gateway_status: ON,
      list_agent_models: [],
    });
    render(<SettingsAgents />);

    await screen.findByText("Active");
    expect(screen.queryByText("Cloudflare AI Gateway")).not.toBeInTheDocument();
  });
});
