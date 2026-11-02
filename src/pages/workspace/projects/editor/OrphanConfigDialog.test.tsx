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
  has_unreadable_profile: false,
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
    expect(screen.getByText("MCP servers: can hold a stored sign-in or stored values")).toBeInTheDocument();
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

  it("puts read-only first after a failed check or with unchecked names", () => {
    const firstAdd = () =>
      screen.getAllByRole("button").map((b) => b.textContent).find((t) => t?.startsWith("Add "));

    const { unmount } = render(
      <OrphanConfigDialog
        directory="/repo"
        existingName="cloned"
        libraryUse={null}
        busy={false}
        error={null}
        onImport={vi.fn()}
        onStartFresh={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(firstAdd()).toBe("Add in read-only mode");
    unmount();

    renderDialog({
      kind: "Readable",
      read_only_hint: false,
      has_unreadable_profile: false,
      groups: [{ kind: "hook", total: 0, flagged_total: 0, names: [], not_checked: 4 }],
    });
    expect(firstAdd()).toBe("Add in read-only mode");
    expect(screen.getByText(/4 were not checked/)).toBeInTheDocument();
  });

  it("refuses to add a config it cannot read", () => {
    renderDialog({ kind: "Unreadable" });

    expect(screen.getByText(/cannot be read/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add normally" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add in read-only mode" })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Remove and start fresh/ })).toBeEnabled();
  });

  it("says profiles wait for an attach only when the config names one", () => {
    const note = "Profiles do not apply until you attach them in the project.";
    const { unmount } = render(
      <OrphanConfigDialog
        directory="/repo"
        existingName="cloned"
        libraryUse={{
          kind: "Readable",
          read_only_hint: false,
          has_unreadable_profile: false,
          groups: [{ kind: "profile", total: 1, flagged_total: 0, names: [{ name: "team", flagged: false }], not_checked: 0 }],
        }}
        busy={false}
        error={null}
        onImport={vi.fn()}
        onStartFresh={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText(note)).toBeInTheDocument();
    unmount();

    renderDialog(readable(false));
    expect(screen.queryByText(note)).not.toBeInTheDocument();
  });

  it("says so when the check failed, and still offers both actions", () => {
    renderDialog(null);

    expect(screen.getByText(/could not check/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add normally" })).toBeEnabled();
  });
});
