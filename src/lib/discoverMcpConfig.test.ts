import { describe, expect, it } from "vitest";
import { buildDiscoverMcpConfig, resolveLocalCommand, type DiscoverMcpSource } from "./discoverMcpConfig";

const base: DiscoverMcpSource = {
  provider: "Example",
  repository_url: null,
  remote: null,
  local: null,
  auth: { env_vars: [] },
};

describe("resolveLocalCommand", () => {
  it("prefers the explicit args array", () => {
    expect(resolveLocalCommand({ command: "docker", args: ["run", "-i", "img"] })).toEqual({
      command: "docker",
      args: ["run", "-i", "img"],
    });
  });

  it("splits a legacy command string", () => {
    expect(resolveLocalCommand({ command: "npx -y pkg" })).toEqual({
      command: "npx",
      args: ["-y", "pkg"],
    });
  });
});

describe("buildDiscoverMcpConfig", () => {
  it("prefers remote when an entry has both, without the local env vars", () => {
    const cfg = buildDiscoverMcpConfig({
      ...base,
      remote: { transport: "streamable-http", url: "https://example.com/mcp" },
      local: { command: "docker", args: ["run", "-i", "--rm", "-e", "TOKEN", "img:1"] },
      auth: { env_vars: [{ name: "TOKEN" }] },
    });
    expect(cfg).toEqual({ type: "http", url: "https://example.com/mcp", _author: { name: "Example" } });
  });

  it("keeps env vars on a remote-only entry", () => {
    const cfg = buildDiscoverMcpConfig({
      ...base,
      remote: { transport: "sse", url: "https://example.com/sse" },
      auth: { env_vars: [{ name: "KEY" }] },
    });
    expect(cfg?.type).toBe("sse");
    expect(cfg?.env).toEqual({ KEY: "" });
  });

  it("returns null for an entry that needs manual setup", () => {
    expect(buildDiscoverMcpConfig(base)).toBeNull();
  });

  it("builds a stdio config for a local-only entry", () => {
    const cfg = buildDiscoverMcpConfig({
      ...base,
      repository_url: "https://github.com/example/server",
      local: { command: "npx -y pkg" },
      auth: { env_vars: [{ name: "KEY" }] },
    });
    expect(cfg).toEqual({
      type: "stdio",
      command: "npx",
      args: ["-y", "pkg"],
      env: { KEY: "" },
      _author: { name: "Example", repository_url: "https://github.com/example/server" },
    });
  });
});
