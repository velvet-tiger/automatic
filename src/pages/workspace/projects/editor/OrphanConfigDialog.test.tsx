import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { OrphanConfigDialog } from "./OrphanConfigDialog";

function renderDialog(profiles: string[]) {
  render(
    <OrphanConfigDialog
      directory="/tmp/app"
      existingName="app"
      profiles={profiles}
      busy={false}
      error={null}
      onImport={vi.fn()}
      onStartFresh={vi.fn()}
      onCancel={vi.fn()}
    />,
  );
}

describe("OrphanConfigDialog", () => {
  it("lists the profiles the config asks for", () => {
    renderDialog(["team", "baseline"]);
    expect(screen.getByText("team, baseline")).toBeInTheDocument();
    expect(screen.getByText("These do not apply until you attach them in the project.")).toBeInTheDocument();
  });

  it("leaves the section out when the config names none", () => {
    renderDialog([]);
    expect(screen.queryByText("Profiles it asks for")).not.toBeInTheDocument();
  });
});
