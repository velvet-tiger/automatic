import { describe, it, expect } from "vitest";
import { groupAgentRemovalEntries, parseAgentRemovalEntries } from "./agentRemoval";

const entry = {
  path: "/p/.claude",
  relative_path: ".claude",
  action: "delete",
  is_dir: true,
  shared_with: [],
};

describe("parseAgentRemovalEntries", () => {
  it("parses a well-formed preview", () => {
    expect(parseAgentRemovalEntries(JSON.stringify([entry]))).toEqual([entry]);
  });

  it("rejects a non-list and an unknown action", () => {
    expect(() => parseAgentRemovalEntries("{}")).toThrow();
    expect(() => parseAgentRemovalEntries(JSON.stringify([{ ...entry, action: "wipe" }]))).toThrow();
  });

  it("rejects the old list-of-paths shape", () => {
    expect(() => parseAgentRemovalEntries(JSON.stringify(["/p/.claude"]))).toThrow();
  });
});

describe("groupAgentRemovalEntries", () => {
  it("splits deletions, strips and shared files", () => {
    const pruned = { ...entry, path: "/p/.agents", relative_path: ".agents", action: "remove_empty_dir" as const };
    const stripped = { ...entry, path: "/p/.vscode/mcp.json", relative_path: ".vscode/mcp.json", action: "strip" as const, is_dir: false };
    const kept = { ...entry, path: "/p/AGENTS.md", relative_path: "AGENTS.md", action: "keep_shared" as const, is_dir: false, shared_with: ["Codex CLI"] };
    const deleted = { ...entry, action: "delete" as const };

    const groups = groupAgentRemovalEntries([deleted, pruned, stripped, kept]);

    expect(groups.deleted).toEqual([deleted, pruned]);
    expect(groups.stripped).toEqual([stripped]);
    expect(groups.kept).toEqual([kept]);
  });
});
