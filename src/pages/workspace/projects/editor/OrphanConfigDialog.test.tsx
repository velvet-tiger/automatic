import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { OrphanConfigDialog } from "./OrphanConfigDialog";
import type { ProjectLibraryUse } from "../../../../lib/libraryUse";

const renderDialog = (libraryUse: ProjectLibraryUse | null, onImport = vi.fn()) => {
  render(
    <OrphanConfigDialog
      directory="/repo"
      existingName="cloned"
      libraryUse={libraryUse}
      busy={false}
      error={null}
      onImport={onImport}
      onStartFresh={vi.fn()}
      onCancel={vi.fn()}
    />,
  );
  return onImport;
};

const readable = (read_only_hint: boolean): ProjectLibraryUse => ({
  kind: "Readable",
  read_only_hint,
  groups: [
    {
      kind: "mcp_server",
      total: 52,
      flagged_total: 1,
      names: [
        { name: "github", flagged: true },
        { name: "<img src=x>", flagged: false },
      ],
      not_checked: 0,
    },
  ],
});

describe("OrphanConfigDialog", () => {
  it("lists library items, flags risky ones and renders names as text", () => {
    renderDialog(readable(false));

    expect(screen.getAllByText("github").length).toBeGreaterThan(0);
    expect(screen.getByText("MCP servers: has stored values")).toBeInTheDocument();
    expect(screen.getByText("<img src=x>")).toBeInTheDocument();
    expect(screen.getByText("and 50 more")).toBeInTheDocument();
  });

  it("passes the chosen mode to the import", async () => {
    const user = userEvent.setup();
    const onImport = renderDialog(readable(false));

    await user.click(screen.getByRole("button", { name: "Add in read-only mode" }));
    expect(onImport).toHaveBeenLastCalledWith(true);
    await user.click(screen.getByRole("button", { name: "Add normally" }));
    expect(onImport).toHaveBeenLastCalledWith(false);
  });

  it("puts read-only first when the committed hint asks for it", () => {
    renderDialog(readable(true));

    const labels = screen.getAllByRole("button").map((b) => b.textContent);
    expect(labels.indexOf("Add in read-only mode")).toBeLessThan(labels.indexOf("Add normally"));
  });

  it("refuses to add a config it cannot read", () => {
    renderDialog({ kind: "Unreadable" });

    expect(screen.getByText(/cannot be read/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add normally" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add in read-only mode" })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Remove and start fresh/ })).toBeEnabled();
  });

  it("says so when the check failed, and still offers both actions", () => {
    renderDialog(null);

    expect(screen.getByText(/could not check/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add normally" })).toBeEnabled();
  });
});
