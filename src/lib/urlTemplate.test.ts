import { describe, expect, it } from "vitest";
import {
  fillUrlTemplate,
  knownUrlTemplates,
  parseUrlTemplate,
  readUrlTemplateValues,
  savedUrlTemplate,
  urlTemplateTokenLabel,
  urlTemplateTokens,
  urlTemplateValueProblem,
} from "./urlTemplate";

const SPACELIFT = "https://<account-name>.app.spacelift.io/mcp";

describe("urlTemplateTokens", () => {
  it("lists each token once, in order", () => {
    expect(urlTemplateTokens("https://<a>.x.io/<b>/<a>")).toEqual(["a", "b"]);
  });

  it("finds the Spacelift token", () => {
    expect(urlTemplateTokens(SPACELIFT)).toEqual(["account-name"]);
  });

  it("ignores a plain URL", () => {
    expect(urlTemplateTokens("https://example.com/mcp")).toEqual([]);
  });
});

describe("parseUrlTemplate", () => {
  it("accepts the Spacelift template", () => {
    expect(parseUrlTemplate(SPACELIFT)?.tokens).toEqual(["account-name"]);
  });

  it.each([
    ["no token", "https://app.spacelift.io/mcp"],
    ["token inside a label", "https://app.spacelift.io<x>/mcp"],
    ["token as the whole host", "https://<x>/mcp"],
    ["token among the last two labels", "https://app.<x>.io/mcp"],
    ["token in the path", "https://app.spacelift.io/<x>"],
    ["repeated token", "https://<a>.<a>.spacelift.io/mcp"],
    ["more than four tokens", "https://<a>.<b>.<c>.<d>.<e>.x.io/mcp"],
    ["user info in the host", "https://user@<a>.x.io/mcp"],
    ["not http", "ftp://<a>.x.io/mcp"],
    ["too long", `https://<a>.x.io/${"p".repeat(600)}`],
  ])("refuses %s", (_, template) => {
    expect(parseUrlTemplate(template)).toBeNull();
  });
});

describe("urlTemplateTokenLabel", () => {
  it("turns hyphens into words", () => {
    expect(urlTemplateTokenLabel("account-name")).toBe("Account name");
  });
});

describe("urlTemplateValueProblem", () => {
  it("accepts a DNS label", () => {
    expect(urlTemplateValueProblem("account-name", "acme-prod2")).toBeNull();
  });

  it.each([
    ["", "Enter your account name."],
    ["evil.com#", expect.stringContaining("letters, digits and hyphens")],
    ["a/b", expect.stringContaining("letters")],
    ["user@evil", expect.stringContaining("letters")],
    ["-acme", expect.stringContaining("hyphen")],
    ["acme-", expect.stringContaining("hyphen")],
    ["a".repeat(64), expect.stringContaining("letters")],
  ])("refuses %j", (value, problem) => {
    expect(urlTemplateValueProblem("account-name", value)).toEqual(problem);
  });
});

describe("fillUrlTemplate", () => {
  it("fills every token and trims the value", () => {
    expect(fillUrlTemplate(SPACELIFT, { "account-name": "  acme " })).toEqual({
      url: "https://acme.app.spacelift.io/mcp",
    });
  });

  it("reports a missing value instead of leaving the token", () => {
    expect(fillUrlTemplate(SPACELIFT, {})).toEqual({ problem: "Enter your account name." });
  });

  it("refuses a value that would change the host", () => {
    expect(fillUrlTemplate(SPACELIFT, { "account-name": "evil.com/" })).toHaveProperty("problem");
  });

  it("refuses to fill a template that is not valid", () => {
    expect(fillUrlTemplate("https://<x>/mcp", { x: "evil" })).toEqual({
      problem: "This server's address template is not valid.",
    });
  });
});

describe("readUrlTemplateValues", () => {
  it("reads the value back from a filled URL", () => {
    expect(readUrlTemplateValues(SPACELIFT, "https://acme.app.spacelift.io/mcp")).toEqual({
      "account-name": "acme",
    });
  });

  it("gives empty values when the URL does not fit the template", () => {
    expect(readUrlTemplateValues(SPACELIFT, "https://acme.example.com/mcp")).toEqual({
      "account-name": "",
    });
  });

  it("gives empty values for the unfilled template", () => {
    expect(readUrlTemplateValues(SPACELIFT, SPACELIFT)).toEqual({ "account-name": "" });
  });

  it("ignores case in the fixed labels", () => {
    expect(readUrlTemplateValues(SPACELIFT, "https://Acme.APP.spacelift.io/mcp")).toEqual({
      "account-name": "Acme",
    });
  });

  it("stays fast on a hostile template and URL", () => {
    // Adjacent tokens made the old RegExp-based reader take seconds.
    const template = `https://${Array.from({ length: 10 }, (_, i) => `<t${i}>`).join("")}.x.io/mcp`;
    const started = performance.now();
    readUrlTemplateValues(template, `https://${"a".repeat(5000)}!`);
    readUrlTemplateValues("https://<a>.<b>.<c>.<d>.x.io/mcp", `https://${"a-".repeat(5000)}.x.io/mcp!`);
    expect(performance.now() - started).toBeLessThan(50);
  });
});

describe("savedUrlTemplate", () => {
  const known = [SPACELIFT];

  it("prefers the recorded template", () => {
    expect(savedUrlTemplate(SPACELIFT, "https://acme.app.spacelift.io/mcp", known)).toBe(SPACELIFT);
  });

  it("treats a URL that is still a known template as the template", () => {
    expect(savedUrlTemplate(undefined, SPACELIFT, known)).toBe(SPACELIFT);
  });

  it("ignores a recorded template that is not in the catalogue", () => {
    const foreign = "https://<account-name>.app.spacelift.io.evil.example/mcp";
    expect(savedUrlTemplate(foreign, "https://acme.app.spacelift.io/mcp", known)).toBeNull();
    expect(savedUrlTemplate(undefined, foreign, known)).toBeNull();
  });

  it("returns null for an ordinary server", () => {
    expect(savedUrlTemplate(undefined, "https://example.com/mcp", known)).toBeNull();
  });
});

describe("knownUrlTemplates", () => {
  it("keeps only valid templates", () => {
    expect(knownUrlTemplates([SPACELIFT, "https://example.com/mcp", null, undefined, "https://<x>/mcp"])).toEqual([
      SPACELIFT,
    ]);
  });
});
