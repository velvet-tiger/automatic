import { describe, expect, it } from "vitest";
import { matchDiscoverEntry, tokenLinksFor, type DiscoverCatalogueEntry } from "./discoverMcpMatch";

function entry(overrides: Partial<DiscoverCatalogueEntry>): DiscoverCatalogueEntry {
  return {
    slug: "example",
    title: "Example",
    description: "An example server.",
    provider: "Example Inc",
    classification: "official",
    repository_url: null,
    remote: null,
    local: null,
    auth: { env_vars: [] },
    ...overrides,
  };
}

const local = entry({
  slug: "aikido-security",
  local: { command: "npx", args: ["-y", "@aikidosec/mcp@latest"] },
  auth: {
    env_vars: [{ name: "AIKIDO_API_KEY", token_url: "https://example.test/token", token_url_label: "Generate token" }],
  },
});
const remote = entry({ slug: "linear", remote: { transport: "http", url: "https://mcp.linear.app/mcp" } });
const templated = entry({
  slug: "spacelift",
  remote: { transport: "http", url: "https://<account-name>.app.spacelift.io/mcp" },
});
const catalogue = [local, remote, templated];

describe("matchDiscoverEntry", () => {
  it("matches a local server by command and arguments, even when renamed", () => {
    const config = { type: "stdio", command: "npx", args: ["-y", "@aikidosec/mcp@latest"] };
    expect(matchDiscoverEntry("aikido-security", config, catalogue)).toBe(local);
    expect(matchDiscoverEntry("aikido-work", config, catalogue)).toBe(local);
  });

  it("does not match on name or author alone", () => {
    const config = { type: "stdio", command: "npx", args: ["-y", "evil-package"] };
    expect(matchDiscoverEntry("aikido-security", config, catalogue)).toBeNull();
    expect(matchDiscoverEntry("linear", { type: "http", url: "https://evil.test/mcp" }, catalogue)).toBeNull();
  });

  it("matches a remote server by URL", () => {
    expect(matchDiscoverEntry("anything", { type: "http", url: "https://mcp.linear.app/mcp" }, catalogue)).toBe(remote);
  });

  it("matches a templated server by its saved template", () => {
    const config = {
      type: "http",
      url: "https://acme.app.spacelift.io/mcp",
      _url_template: "https://<account-name>.app.spacelift.io/mcp",
    };
    expect(matchDiscoverEntry("spacelift", config, catalogue)).toBe(templated);
  });

  it("does not match a copied template whose URL points at another host", () => {
    const template = "https://<account-name>.app.spacelift.io/mcp";
    for (const url of [
      "https://evil.test/mcp",
      "https://acme.evil.test/mcp",
      "https://acme.app.spacelift.io.evil.test/mcp",
      "https://acme.app.spacelift.io/other",
      "https://a.b.app.spacelift.io/mcp",
    ]) {
      expect(matchDiscoverEntry("spacelift", { type: "http", url, _url_template: template }, catalogue)).toBeNull();
    }
  });

  it("prefers the entry whose slug is the server name when several match", () => {
    const twin = entry({ slug: "linear-twin", remote: remote.remote });
    const config = { type: "http", url: "https://mcp.linear.app/mcp" };
    expect(matchDiscoverEntry("linear-twin", config, [remote, twin])).toBe(twin);
  });
});

describe("tokenLinksFor", () => {
  it("lists links only for env vars the config holds", () => {
    expect(tokenLinksFor(local, { AIKIDO_API_KEY: "", OTHER: "" })).toEqual([
      { name: "AIKIDO_API_KEY", url: "https://example.test/token", label: "Generate token" },
    ]);
    expect(tokenLinksFor(null, { AIKIDO_API_KEY: "" })).toEqual([]);
  });
});
