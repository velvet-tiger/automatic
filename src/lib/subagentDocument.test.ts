import { describe, it, expect } from "vitest";
import {
  conflictingExtraKeys,
  parseSubagent,
  serializeSubagent,
  subagentMachineName,
  type SubagentDocument,
} from "./subagentDocument";

const CLAUDE_AGENT = `---
name: code-reviewer
description: Reviews code for quality. Use PROACTIVELY after code changes.
tools: Read, Glob, Grep, Bash
model: sonnet
color: yellow
---

You are a code reviewer.
`;

const WITH_EXTRAS = `---
# managed by hand
name: "planner"
permissionMode: plan
description: |
  Plans work.
  Never edits files.
hooks:
  PreToolUse:
    - matcher: Bash
      command: ./check.sh
tools:
  - Read
  - Grep
maxTurns: 12
---
Plan first.
`;

function edit(doc: SubagentDocument, patch: Partial<SubagentDocument["fields"]>): SubagentDocument {
  return { ...doc, fields: { ...doc.fields, ...patch } };
}

describe("parseSubagent", () => {
  it("splits known keys into fields and keeps the body", () => {
    const doc = parseSubagent(CLAUDE_AGENT);
    expect(doc.fields).toEqual({
      name: "code-reviewer",
      description: "Reviews code for quality. Use PROACTIVELY after code changes.",
      tools: "Read, Glob, Grep, Bash",
      model: "sonnet",
      color: "yellow",
    });
    expect(doc.extraFrontmatter).toBe("");
    expect(doc.body).toBe("\nYou are a code reviewer.\n");
  });

  it("decodes quoted, block, and list values and keeps other keys as extra", () => {
    const doc = parseSubagent(WITH_EXTRAS);
    expect(doc.fields.name).toBe("planner");
    expect(doc.fields.description).toBe("Plans work.\nNever edits files.");
    expect(doc.fields.tools).toBe("Read, Grep");
    expect(doc.fields.model).toBe("");
    expect(doc.extraFrontmatter).toBe(
      [
        "# managed by hand",
        "permissionMode: plan",
        "hooks:",
        "  PreToolUse:",
        "    - matcher: Bash",
        "      command: ./check.sh",
        "maxTurns: 12",
      ].join("\n"),
    );
    expect(doc.body).toBe("Plan first.\n");
  });

  it("treats content without front matter as all body", () => {
    const doc = parseSubagent("Just a prompt.\n");
    expect(doc.fields.name).toBe("");
    expect(doc.body).toBe("Just a prompt.\n");
  });

  it("keeps a field key with a nested value as extra", () => {
    const doc = parseSubagent("---\nname: a\nmodel:\n  id: x\n---\nbody");
    expect(doc.fields.model).toBe("");
    expect(doc.extraFrontmatter).toBe("model:\n  id: x");
  });
});

describe("serializeSubagent round trip", () => {
  const samples: Record<string, string> = {
    claude: CLAUDE_AGENT,
    extras: WITH_EXTRAS,
    crlf: "---\r\nname: a\r\ndescription: b\r\nfoo: bar\r\n---\r\nbody\r\n",
    noFrontmatter: "No front matter here.\n",
    emptyBody: "---\nname: a\n---",
    duplicateKey: "---\nname: a\nname: b\n---\nbody\n",
    blankLines: "---\nname: a\n\ndescription: b\n\n---\n\nbody",
  };

  for (const [label, content] of Object.entries(samples)) {
    it(`returns ${label} unchanged when nothing is edited`, () => {
      expect(serializeSubagent(parseSubagent(content))).toBe(content);
    });
  }

  it("rewrites only the edited field and keeps everything else verbatim", () => {
    const doc = edit(parseSubagent(WITH_EXTRAS), { name: "Planner v2" });
    expect(serializeSubagent(doc)).toBe(WITH_EXTRAS.replace('name: "planner"', "name: Planner v2"));
  });

  it("adds a newly filled field after the existing keys", () => {
    const doc = edit(parseSubagent(WITH_EXTRAS), { model: "opus" });
    expect(serializeSubagent(doc)).toBe(WITH_EXTRAS.replace("maxTurns: 12\n", "maxTurns: 12\nmodel: opus\n"));
  });

  it("removes a field that is cleared", () => {
    const doc = edit(parseSubagent(CLAUDE_AGENT), { color: "" });
    expect(serializeSubagent(doc)).toBe(CLAUDE_AGENT.replace("color: yellow\n", ""));
  });

  it("quotes values YAML would misread", () => {
    const doc = edit(parseSubagent(CLAUDE_AGENT), {
      description: 'Use when: "reviewing"\nsecond line',
      model: "true",
    });
    const out = serializeSubagent(doc);
    expect(out).toContain('description: "Use when: \\"reviewing\\"\\nsecond line"\n');
    expect(out).toContain('model: "true"\n');
    expect(parseSubagent(out).fields.description).toBe('Use when: "reviewing"\nsecond line');
  });

  it("appends edited extra front matter after the fields", () => {
    const doc = { ...parseSubagent(CLAUDE_AGENT), extraFrontmatter: "permissionMode: plan\n" };
    expect(serializeSubagent(doc)).toBe(CLAUDE_AGENT.replace("color: yellow\n", "color: yellow\npermissionMode: plan\n"));
  });

  it("builds front matter for a file that had none", () => {
    const doc = edit(parseSubagent("Prompt.\n"), { name: "helper", description: "Helps." });
    expect(serializeSubagent(doc)).toBe("---\nname: helper\ndescription: Helps.\n---\nPrompt.\n");
  });

  it("adds a line break before a new body when the file ended at the closing line", () => {
    const doc = { ...parseSubagent("---\nname: a\n---"), body: "Hello" };
    expect(serializeSubagent(doc)).toBe("---\nname: a\n---\nHello");
  });
});

describe("conflictingExtraKeys", () => {
  it("reports field keys that also appear in the extra front matter", () => {
    const doc = { ...parseSubagent(CLAUDE_AGENT), extraFrontmatter: "model: opus\nhooks: {}" };
    expect(conflictingExtraKeys(doc)).toEqual(["model"]);
  });

  it("ignores a field key in extra when the field itself is empty", () => {
    const doc = parseSubagent("---\nname: a\nmodel:\n  id: x\n---\n");
    expect(conflictingExtraKeys(doc)).toEqual([]);
  });
});

describe("subagentMachineName", () => {
  it("slugifies a display name", () => {
    expect(subagentMachineName("Code Reviewer!")).toBe("code-reviewer");
  });

  it("starts with a letter", () => {
    expect(subagentMachineName("42 things")).toBe("agent-42-things");
    expect(subagentMachineName("   ")).toBe("agent");
  });

  it("never claims a read-only prefix", () => {
    expect(subagentMachineName("Automatic Reviewer")).toBe("reviewer");
    expect(subagentMachineName("Codex Helper OpenAI")).toBe("codex-helper-openai-agent");
  });
});
