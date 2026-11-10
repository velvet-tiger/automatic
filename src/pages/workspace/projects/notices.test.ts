import { describe, it, expect, beforeEach } from "vitest";
import {
  collectProjectNotices,
  countNotices,
  readHiddenNotices,
  staysHidden,
  writeHiddenNotices,
  type ProjectNoticeInputs,
} from "./notices";
import type { Project, ProjectProblem } from "./types";

function project(overrides: Partial<Project> = {}): Project {
  return {
    name: "app",
    description: "",
    directory: "/tmp/app",
    skills: [],
    mcp_servers: [],
    providers: [],
    agents: ["claude"],
    created_at: "",
    updated_at: "",
    ...overrides,
  } as Project;
}

const problem: ProjectProblem = {
  kind: "mcp_user_scope_conflict",
  title: "github is also set up at user scope",
  description: "Claude Code loads both copies.",
  agents: ["Claude Code"],
  resources: ["github"],
};

function inputs(overrides: Partial<ProjectNoticeInputs> = {}): ProjectNoticeInputs {
  return {
    project: project(),
    selectedKey: "app",
    isCreating: false,
    dirty: false,
    readOnly: false,
    driftReport: null,
    driftCheckError: null,
    problemsReport: null,
    problemsCheckError: null,
    checksKey: "app",
    ...overrides,
  };
}

const ids = (i: ProjectNoticeInputs) => collectProjectNotices(i).map((n) => n.id);

describe("collectProjectNotices", () => {
  it("is empty for a healthy project", () => {
    expect(collectProjectNotices(inputs())).toEqual([]);
  });

  it("shows nothing while a project is being created", () => {
    expect(ids(inputs({ isCreating: true, project: project({ directory_missing: true, mode: "silent" }) }))).toEqual([]);
  });

  it("sorts problems, then warnings, then notices", () => {
    const result = collectProjectNotices(
      inputs({
        project: project({ mode: "silent", pending_contexts: ["docs"] }),
        driftReport: { drifted: true, agents: [] },
        problemsReport: { has_problems: true, problems: [problem], notes: ["Codex: dropped a tool."] },
      }),
    );
    expect(result.map((n) => [n.id.split(":")[0], n.severity])).toEqual([
      ["problem", "problem"],
      ["drift", "warning"],
      ["pending_contexts", "warning"],
      ["silent_mode", "notice"],
      ["sync_notes", "notice"],
    ]);
  });

  it("holds back problems and sync notes while there are unsaved edits", () => {
    const result = ids(
      inputs({
        dirty: true,
        driftReport: { drifted: true, agents: [] },
        problemsReport: { has_problems: true, problems: [problem], notes: ["a note"] },
      }),
    );
    expect(result).toEqual(["drift"]);
  });

  it("shows a failed check even with unsaved edits", () => {
    expect(ids(inputs({ dirty: true, driftCheckError: "locked" }))).toEqual([
      "check_failure:Could not check this project for changes.",
    ]);
  });

  it("merges one error shared by both checks into one notice", () => {
    expect(collectProjectNotices(inputs({ driftCheckError: "e", problemsCheckError: "e" }))).toHaveLength(1);
  });

  it("skips the checks for a project with no folder or no agents", () => {
    const failing = { driftCheckError: "e", driftReport: { drifted: true, agents: [] } };
    expect(ids(inputs({ ...failing, project: project({ agents: [] }) }))).toEqual([]);
    expect(ids(inputs({ ...failing, project: project({ directory: "" }) }))).toEqual([]);
  });

  it("ignores check results that belong to the previous project", () => {
    const stale = inputs({
      checksKey: "previous",
      driftReport: { drifted: true, agents: [] },
      driftCheckError: "e",
      problemsReport: { has_problems: true, problems: [problem], notes: ["n"] },
    });
    expect(ids(stale)).toEqual([]);
    expect(ids({ ...stale, project: project({ mode: "silent" }) })).toEqual(["silent_mode"]);
  });

  it("reports a missing folder as a problem", () => {
    const [notice] = collectProjectNotices(inputs({ project: project({ directory_missing: true }) }));
    expect(notice).toMatchObject({ id: "missing_directory", severity: "problem", source: { directory: "/tmp/app" } });
  });

  it("counts one row per waiting profile and leaves out ignored ones", () => {
    const [notice] = collectProjectNotices(
      inputs({ project: project({ pending_profiles: ["a", "b", "c"], local_ignored_profiles: ["b"] }) }),
    );
    expect(notice).toMatchObject({ id: "pending_profiles", count: 2, source: { names: ["a", "c"] } });
  });

  it("does not ask about profiles on a read-only project", () => {
    expect(ids(inputs({ readOnly: true, project: project({ pending_profiles: ["a"] }) }))).toEqual([]);
  });

  it("asks about a registration only for a stored project", () => {
    const awaiting = project({ local_awaiting_confirmation: true });
    expect(ids(inputs({ project: awaiting }))).toEqual(["pending_registration"]);
    expect(ids(inputs({ project: awaiting, selectedKey: null }))).toEqual([]);
  });

  it("keeps a problem's id when another problem is fixed", () => {
    const other = { ...problem, kind: "agent_sync_limits" as const, title: "Sub-agents not written" };
    const before = ids(inputs({ problemsReport: { has_problems: true, problems: [problem, other] } }));
    const after = ids(inputs({ problemsReport: { has_problems: true, problems: [other] } }));
    expect(after).toEqual([before[1]]);
  });

  it("keeps a problem's id when its title changes between singular and plural", () => {
    const one = { ...problem, title: "MCP server also configured at user scope" };
    const two = { ...problem, title: "MCP servers also configured at user scope" };
    expect(ids(inputs({ problemsReport: { has_problems: true, problems: [one] } }))).toEqual(
      ids(inputs({ problemsReport: { has_problems: true, problems: [two] } })),
    );
  });

  it("gives each problem its own id", () => {
    const report = { has_problems: true, problems: [problem, { ...problem, title: "other" }] };
    const result = ids(inputs({ problemsReport: report }));
    expect(new Set(result).size).toBe(2);
  });
});

describe("collectProjectNotices: writable folders target", () => {
  const target = {
    folders: ["/Users/me/work/cli"],
    reason: "tracked" as const,
    paste_json: '{"permissions":{"additionalDirectories":["/Users/me/work/cli"]}}',
  };
  const report = { has_problems: false, problems: [], writable_folders_target: target };

  it("asks as a warning when the report carries the decision", () => {
    const result = collectProjectNotices(inputs({ problemsReport: report }));
    expect(result).toEqual([
      {
        id: "writable_folders_target",
        severity: "warning",
        count: 1,
        source: { kind: "writable_folders_target", notice: target },
      },
    ]);
  });

  it("waits while there are unsaved edits", () => {
    expect(ids(inputs({ dirty: true, problemsReport: report }))).toEqual([]);
  });

  it("is skipped for a project without agents", () => {
    expect(ids(inputs({ project: project({ agents: [] }), problemsReport: report }))).toEqual([]);
  });

  it("ignores a decision from the previous project", () => {
    expect(ids(inputs({ checksKey: "other", problemsReport: report }))).toEqual([]);
  });

  it("is absent when the report carries no decision", () => {
    expect(ids(inputs({ problemsReport: { has_problems: false, problems: [] } }))).toEqual([]);
  });
});

describe("countNotices", () => {
  it("adds up rows by severity", () => {
    const notices = collectProjectNotices(
      inputs({
        project: project({ pending_profiles: ["a", "b"], mode: "silent" }),
        problemsReport: { has_problems: true, problems: [problem] },
      }),
    );
    expect(countNotices(notices)).toEqual({ problem: 1, warning: 2, notice: 1 });
  });
});

describe("staysHidden", () => {
  const withProblem = collectProjectNotices(inputs({ problemsReport: { has_problems: true, problems: [problem] } }));
  const problemId = withProblem[0]!.id;

  it("stays hidden while every problem was already seen", () => {
    expect(staysHidden([problemId], withProblem)).toBe(true);
    expect(staysHidden([problemId], [])).toBe(true);
  });

  it("opens again for a problem the user has not seen", () => {
    expect(staysHidden([], withProblem)).toBe(false);
  });

  it("ignores new warnings and notices", () => {
    const warnings = collectProjectNotices(inputs({ driftReport: { drifted: true, agents: [] } }));
    expect(staysHidden([], warnings)).toBe(true);
  });
});

describe("hidden state storage", () => {
  beforeEach(() => localStorage.clear());

  it("round-trips per project", () => {
    writeHiddenNotices("app", ["problem:x"]);
    expect(readHiddenNotices("app")).toEqual(["problem:x"]);
    expect(readHiddenNotices("other")).toBeNull();
    writeHiddenNotices("app", null);
    expect(readHiddenNotices("app")).toBeNull();
  });

  it("treats a malformed value as open", () => {
    localStorage.setItem("automatic.project_notices.hidden.app", "{not json");
    expect(readHiddenNotices("app")).toBeNull();
    localStorage.setItem("automatic.project_notices.hidden.app", JSON.stringify([1, 2]));
    expect(readHiddenNotices("app")).toBeNull();
  });
});
