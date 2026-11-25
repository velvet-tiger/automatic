/**
 * A catalogue server whose env vars are all optional signs in some other way
 * (for example through the browser). The Authentication tab must say so, and
 * must not claim the variables are required.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock } from "../../test/tauriMock";
import { render, screen, fireEvent } from "@testing-library/react";
import DiscoverMcp from "./DiscoverMcp";

const NOTE = "Sign in through the browser on first use.";

function server(optional: boolean, note: string | null) {
  return {
    slug: "example-local",
    name: "dev.example/mcp",
    title: "Example Local",
    description: "A local server.",
    provider: "Example",
    classification: "official",
    repository_url: null,
    remote: null,
    local: {
      registry: "npm",
      package: "@example/mcp",
      version: "latest",
      transport: "stdio",
      command: "npx",
      args: ["-y", "@example/mcp@latest"],
    },
    auth: {
      method: "api_key",
      note,
      env_vars: [{ name: "EXAMPLE_API_KEY", description: "Token.", secret: true, optional }],
    },
  };
}

function routes(entry: ReturnType<typeof server>) {
  return {
    search_discover_mcp: JSON.stringify([entry]),
    list_mcp_server_configs: [],
    get_skills: [],
    validate_mcp_server_config: { findings: [], docker_fix_available: false, remote_equivalent: null },
  };
}

async function openAuthTab() {
  render(<DiscoverMcp initialSlug="example-local" />);
  fireEvent.click(await screen.findByRole("button", { name: /Authentication/ }));
}

describe("DiscoverMcp optional auth", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("shows the sign-in note and an optional badge", async () => {
    mockInvoke(routes(server(true, NOTE)));
    await openAuthTab();

    expect(screen.getByText(NOTE)).toBeInTheDocument();
    expect(screen.getByText("optional")).toBeInTheDocument();
    expect(screen.queryByText(/This server requires authentication/)).not.toBeInTheDocument();
  });

  it("falls back to a generic line when there is no note", async () => {
    mockInvoke(routes(server(true, null)));
    await openAuthTab();

    expect(screen.getByText(/These environment variables are optional/)).toBeInTheDocument();
  });

  it("still says required when a variable is not optional", async () => {
    mockInvoke(routes(server(false, NOTE)));
    await openAuthTab();

    expect(screen.getByText(/This server requires authentication/)).toBeInTheDocument();
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
    expect(screen.queryByText("optional")).not.toBeInTheDocument();
  });
});
