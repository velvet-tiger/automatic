import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { LibraryUseList } from "./LibraryUseList";

describe("LibraryUseList", () => {
  afterEach(() => vi.restoreAllMocks());

  it("warns about names it did not check", () => {
    render(
      <LibraryUseList
        groups={[{ kind: "skill", total: 1, flagged_total: 0, names: [{ name: "a", flagged: false }], not_checked: 7 }]}
      />,
    );

    expect(screen.getByText(/lists too many names to check. 7 were not checked/)).toBeInTheDocument();
  });

  it("renders the same name twice without a duplicate key", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});

    render(
      <LibraryUseList
        groups={[
          {
            kind: "skill",
            total: 2,
            flagged_total: 2,
            names: [
              { name: "same�", flagged: true },
              { name: "same�", flagged: true },
            ],
            not_checked: 0,
          },
        ]}
      />,
    );

    expect(screen.getAllByText("same�")).toHaveLength(4);
    const keyErrors = consoleError.mock.calls.filter((call) => String(call[0]).includes("same key"));
    expect(keyErrors).toEqual([]);
  });
});
