import { describe, expect, it } from "vitest";
import { inSaveOrder, takeSaveTurn } from "./projectSaves";

function deferred() {
  let resolve: () => void = () => {};
  const promise = new Promise<void>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("project save order", () => {
  it("runs saves for one key in the order they were made", async () => {
    const order: string[] = [];
    const first = deferred();

    const a = inSaveOrder("k", async () => {
      await first.promise;
      order.push("a");
    });
    const b = inSaveOrder("k", async () => {
      order.push("b");
    });
    first.resolve();
    await Promise.all([a, b]);

    expect(order).toEqual(["a", "b"]);
  });

  it("does not hold up another key", async () => {
    const blocked = deferred();
    const a = inSaveOrder("one", () => blocked.promise);

    await expect(inSaveOrder("two", async () => "done")).resolves.toBe("done");

    blocked.resolve();
    await a;
  });

  it("carries on after a failed save and reports the failure", async () => {
    const failed = inSaveOrder("k", async () => {
      throw new Error("refused");
    });
    const next = inSaveOrder("k", async () => "saved");

    await expect(failed).rejects.toThrow("refused");
    await expect(next).resolves.toBe("saved");
  });

  it("holds later saves for a whole turn", async () => {
    const order: string[] = [];
    const release = await takeSaveTurn("k");
    const later = inSaveOrder("k", async () => {
      order.push("later");
    });

    await Promise.resolve();
    order.push("turn");
    release();
    await later;

    expect(order).toEqual(["turn", "later"]);
  });
});
