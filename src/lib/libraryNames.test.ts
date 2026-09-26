import { describe, it, expect } from "vitest";
import { validateLibraryName } from "./libraryNames";

describe("validateLibraryName", () => {
  it("accepts lowercase letters, digits, and hyphens", () => {
    expect(validateLibraryName("code-reviewer")).toBeNull();
    expect(validateLibraryName("agent-2")).toBeNull();
  });

  it("requires a name", () => {
    expect(validateLibraryName("")).toBe("Name is required.");
  });

  it("rejects capitals, spaces, and other characters", () => {
    const charset = "Name may only contain lowercase letters, numbers, and hyphens.";
    expect(validateLibraryName("Code Reviewer")).toBe(charset);
    expect(validateLibraryName("code_reviewer")).toBe(charset);
    expect(validateLibraryName("code.reviewer")).toBe(charset);
  });

  it("rejects names over 64 characters", () => {
    expect(validateLibraryName("a".repeat(64))).toBeNull();
    expect(validateLibraryName("a".repeat(65))).toBe("Name must be 64 characters or fewer.");
  });

  it("rejects reserved words as whole hyphen-separated parts", () => {
    expect(validateLibraryName("claude")).toMatch(/reserved word "claude"/);
    expect(validateLibraryName("my-anthropic-agent")).toMatch(/reserved word "anthropic"/);
    expect(validateLibraryName("claudette")).toBeNull();
  });
});
