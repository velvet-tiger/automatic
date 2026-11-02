import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { LibraryUseList } from "./LibraryUseList";

describe("LibraryUseList", () => {
  it("warns about names it did not check", () => {
    render(
      <LibraryUseList
        groups={[{ kind: "skill", total: 1, flagged_total: 0, names: [{ name: "a", flagged: false }], not_checked: 7 }]}
      />,
    );

    expect(screen.getByText(/lists too many names to check. 7 were not checked/)).toBeInTheDocument();
  });

  it("renders the same name twice without losing a row", () => {
    render(
      <LibraryUseList
        groups={[
          {
            kind: "skill",
            total: 2,
            flagged_total: 0,
            names: [
              { name: "same�", flagged: false },
              { name: "same�", flagged: false },
            ],
            not_checked: 0,
          },
        ]}
      />,
    );

    expect(screen.getAllByText("same�")).toHaveLength(2);
  });
});
