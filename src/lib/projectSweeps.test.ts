import { describe, expect, it } from "vitest";
import { projectSweepMessage, ProjectSweepEvent } from "./projectSweeps";

const event = (overrides: Partial<ProjectSweepEvent>): ProjectSweepEvent => ({
  job_id: 1,
  label: "rule code",
  done: 12,
  total: 58,
  failed: [],
  finished: false,
  duration_ms: 0,
  ...overrides,
});

describe("projectSweepMessage", () => {
  it("counts progress while running", () => {
    expect(projectSweepMessage(event({}))).toBe("Updating projects for rule code: 12 of 58");
  });

  it("gives the total and duration when finished", () => {
    expect(projectSweepMessage(event({ done: 1, total: 1, finished: true, duration_ms: 1500 }))).toBe(
      "Updated 1 project for rule code in 1.5s",
    );
  });

  it("names each failed project and its reason", () => {
    const failed = [
      { project: "site", error: "disk full" },
      { project: "api", error: "refused" },
    ];
    expect(projectSweepMessage(event({ finished: true, failed }))).toBe(
      "Could not update 2 projects for rule code. site: disk full; api: refused",
    );
  });
});
