/**
 * One queue per saved item, so saves reach the backend in the order they were
 * made. `save_project` runs off the main thread and waits on a lock with no
 * order, so an older save could otherwise land last and undo a newer one.
 */
import { invoke } from "@tauri-apps/api/core";

const tails = new Map<string, Promise<void>>();

/**
 * Wait for `key`'s turn. Call the returned function when finished, or every
 * later save for `key` waits for ever. Use it for a turn that spans more than
 * one call; otherwise use {@link inSaveOrder}.
 */
export async function takeSaveTurn(key: string): Promise<() => void> {
  const previous = tails.get(key) ?? Promise.resolve();
  let release: () => void = () => {};
  const tail = new Promise<void>((resolve) => {
    release = resolve;
  });
  tails.set(key, tail);
  await previous;
  return () => {
    if (tails.get(key) === tail) tails.delete(key);
    release();
  };
}

/** Run `save` after every earlier save for `key` has settled. */
export async function inSaveOrder<T>(key: string, save: () => Promise<T>): Promise<T> {
  const release = await takeSaveTurn(key);
  try {
    return await save();
  } finally {
    release();
  }
}

/** Key for a project's turn. Project saves and their follow-up reads share it. */
export function projectSaveKey(name: string): string {
  return `project:${name}`;
}

/** `save_project`, in order with every other save of this project. */
export function saveProject<T = unknown>(name: string, data: string, creating = false): Promise<T> {
  return inSaveOrder(projectSaveKey(name), () =>
    invoke<T>("save_project", { name, data, ...(creating ? { creating: true } : {}) }),
  );
}
