import { vi } from "vitest";

export type InvokeArgs = Record<string, unknown> | undefined;
export type InvokeRoute = unknown | ((args: InvokeArgs) => unknown);

export const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => (invokeMock as unknown as (...args: unknown[]) => unknown)(...a),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: vi.fn().mockResolvedValue(true),
  message: vi.fn().mockResolvedValue(undefined),
  open: vi.fn().mockResolvedValue(null),
  save: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
  openPath: vi.fn().mockResolvedValue(undefined),
}));

/**
 * Route invoke calls by command name to canned responses.
 * Commands not listed resolve to `undefined`.
 */
export function mockInvoke(routes: Record<string, InvokeRoute>): void {
  invokeMock.mockImplementation((cmd: string, args: InvokeArgs) => {
    const r = routes[cmd];
    if (r === undefined) return Promise.resolve(undefined);
    if (typeof r === "function") {
      return Promise.resolve((r as (a: InvokeArgs) => unknown)(args));
    }
    return Promise.resolve(r);
  });
}

export function resetInvokeMock(): void {
  invokeMock.mockReset();
}
