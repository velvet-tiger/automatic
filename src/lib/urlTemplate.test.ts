import { describe, expect, it } from "vitest";
import {
  fillUrlTemplate,
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

  it("requires a repeated token to match the same value", () => {
    const template = "https://<a>.x.io/<a>";
    expect(readUrlTemplateValues(template, "https://one.x.io/one")).toEqual({ a: "one" });
    expect(readUrlTemplateValues(template, "https://one.x.io/two")).toEqual({ a: "" });
  });
});

describe("savedUrlTemplate", () => {
  it("prefers the recorded template", () => {
    expect(savedUrlTemplate(SPACELIFT, "https://acme.app.spacelift.io/mcp")).toBe(SPACELIFT);
  });

  it("treats a URL that still holds a token as the template", () => {
    expect(savedUrlTemplate(undefined, SPACELIFT)).toBe(SPACELIFT);
  });

  it("returns null for an ordinary server", () => {
    expect(savedUrlTemplate(undefined, "https://example.com/mcp")).toBeNull();
    expect(savedUrlTemplate("https://example.com/mcp", "https://example.com/mcp")).toBeNull();
  });
});
