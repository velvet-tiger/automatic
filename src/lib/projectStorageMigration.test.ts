import { describe, it, expect } from "vitest";
import type { ProjectSummary } from "../pages/workspace/projects/types";
import {
  migrateProjectStorageOnce,
  migrateOrder,
  nameToKeyMap,
  planBuildKeyMoves,
  PROJECT_ORDER_STORAGE_KEY,
  PROJECT_STORAGE_VERSION,
  PROJECT_STORAGE_VERSION_KEY,
  SELECTED_PROJECT_STORAGE_KEY,
  type ProjectStorage,
} from "./projectStorageMigration";

/** In-memory Storage, so the tests do not depend on a DOM localStorage. */
class MemoryStorage implements ProjectStorage {
  private readonly data = new Map<string, string>();
  get length(): number {
    return this.data.size;
  }
  key(i: number): string | null {
    return [...this.data.keys()][i] ?? null;
  }
  getItem(k: string): string | null {
    return this.data.get(k) ?? null;
  }
  setItem(k: string, v: string): void {
    this.data.set(k, v);
  }
  removeItem(k: string): void {
    this.data.delete(k);
  }
  snapshot(): Record<string, string> {
    return Object.fromEntries(this.data);
  }
}

const summary = (name: string, local_key: string): ProjectSummary => ({
  name,
  local_key,
  id: `id-${name}`,
  directory: `/work/${name}`,
});

const APP = summary("app", "key-app");
const WEB = summary("web", "key-web");

describe("pure helpers", () => {
  it("maps names to keys and leaves out shared names", () => {
    const map = nameToKeyMap([APP, WEB, summary("web", "key-web-2"), summary("nokey", "")]);
    expect(map.get("app")).toBe("key-app");
    expect(map.has("web")).toBe(false);
    expect(map.has("nokey")).toBe(false);
  });

  it("maps an order, keeps keys and unknowns, drops duplicates", () => {
    const keys = new Set(["key-app", "key-web"]);
    const map = nameToKeyMap([APP, WEB]);
    expect(migrateOrder(["web", "key-app", "gone", "app"], keys, map)).toEqual(["key-web", "key-app", "gone"]);
  });

  it("plans build key moves for names only", () => {
    const keys = new Set(["key-app", "key-web"]);
    const map = nameToKeyMap([APP, WEB]);
    const moves = planBuildKeyMoves(
      [
        "automatic.projects.app.build.view",
        "automatic.projects.key-web.build.sort",
        "automatic.projects.gone.build.filters",
        "automatic.projects.viewMode",
      ],
      keys,
      map,
    );
    expect(moves).toEqual([["automatic.projects.app.build.view", "automatic.projects.key-app.build.view"]]);
  });

  it("handles names that contain dots", () => {
    const dotted = summary("my.app", "key-dotted");
    const moves = planBuildKeyMoves(
      ["automatic.projects.my.app.build.columnWidths"],
      new Set(["key-dotted"]),
      nameToKeyMap([dotted]),
    );
    expect(moves).toEqual([
      ["automatic.projects.my.app.build.columnWidths", "automatic.projects.key-dotted.build.columnWidths"],
    ]);
  });
});

describe("migrateProjectStorageOnce", () => {
  it("moves selected, order and build keys to local keys and sets the flag", () => {
    const s = new MemoryStorage();
    s.setItem(SELECTED_PROJECT_STORAGE_KEY, "web");
    s.setItem(PROJECT_ORDER_STORAGE_KEY, JSON.stringify(["web", "app"]));
    s.setItem("automatic.projects.app.build.view", "list");
    s.setItem("automatic.projects.app.build.filters", '{"filterState":"todo"}');

    expect(migrateProjectStorageOnce(s, [APP, WEB])).toBe(true);

    expect(s.snapshot()).toEqual({
      [SELECTED_PROJECT_STORAGE_KEY]: "key-web",
      [PROJECT_ORDER_STORAGE_KEY]: JSON.stringify(["key-web", "key-app"]),
      "automatic.projects.key-app.build.view": "list",
      "automatic.projects.key-app.build.filters": '{"filterState":"todo"}',
      [PROJECT_STORAGE_VERSION_KEY]: PROJECT_STORAGE_VERSION,
    });
  });

  it("does nothing once the flag is set", () => {
    const s = new MemoryStorage();
    s.setItem(PROJECT_STORAGE_VERSION_KEY, PROJECT_STORAGE_VERSION);
    s.setItem(SELECTED_PROJECT_STORAGE_KEY, "app");
    migrateProjectStorageOnce(s, [APP]);
    expect(s.getItem(SELECTED_PROJECT_STORAGE_KEY)).toBe("app");
  });

  it("is idempotent when run twice without the flag", () => {
    const s = new MemoryStorage();
    s.setItem(SELECTED_PROJECT_STORAGE_KEY, "app");
    const noKey = summary("pending", "");
    expect(migrateProjectStorageOnce(s, [APP, noKey])).toBe(false);
    expect(s.getItem(PROJECT_STORAGE_VERSION_KEY)).toBeNull();
    expect(migrateProjectStorageOnce(s, [APP, noKey])).toBe(false);
    expect(s.getItem(SELECTED_PROJECT_STORAGE_KEY)).toBe("key-app");
  });

  it("keeps a value already stored under the key", () => {
    const s = new MemoryStorage();
    s.setItem("automatic.projects.app.build.view", "board");
    s.setItem("automatic.projects.key-app.build.view", "list");
    migrateProjectStorageOnce(s, [APP]);
    expect(s.getItem("automatic.projects.key-app.build.view")).toBe("list");
    expect(s.getItem("automatic.projects.app.build.view")).toBeNull();
  });

  it("drops a corrupt order", () => {
    const s = new MemoryStorage();
    s.setItem(PROJECT_ORDER_STORAGE_KEY, "{not json");
    migrateProjectStorageOnce(s, [APP]);
    expect(s.getItem(PROJECT_ORDER_STORAGE_KEY)).toBeNull();
  });

  it("leaves unknown selected values alone", () => {
    const s = new MemoryStorage();
    s.setItem(SELECTED_PROJECT_STORAGE_KEY, "deleted-project");
    migrateProjectStorageOnce(s, [APP]);
    expect(s.getItem(SELECTED_PROJECT_STORAGE_KEY)).toBe("deleted-project");
  });
});
