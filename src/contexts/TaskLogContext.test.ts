import { describe, expect, it } from "vitest";
import { fromPersistedEntry, PersistedEntry } from "./TaskLogContext";

const persisted = (status: string): PersistedEntry => ({
  id: "1",
  timestamp: "2026-11-01T00:00:00.000Z",
  message: "Updating projects",
  status,
});

describe("fromPersistedEntry", () => {
  it("marks an entry left running by the last run as interrupted", () => {
    expect(fromPersistedEntry(persisted("running")).status).toBe("interrupted");
  });

  it("keeps every other status", () => {
    expect(fromPersistedEntry(persisted("error")).status).toBe("error");
    expect(fromPersistedEntry(persisted("success")).status).toBe("success");
  });
});
