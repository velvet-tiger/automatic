/**
 * The "Used in N projects" panel on a rule must report what the backend
 * finds on disk. Updating one project must not mark the others in sync.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock } from "../../test/tauriMock";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import Rules from "./Rules";

describe("Rules usage panel", () => {
  let synced: Set<string>;

  beforeEach(() => {
    resetInvokeMock();
    synced = new Set();
    mockInvoke({
      get_rules: [{ id: "team-style", name: "Team style" }],
      read_rule: JSON.stringify({ name: "Team style", content: "Use short names." }),
      get_project_summaries: [],
      scan_asset_content: { blocked: false, findings: [] },
      get_projects_referencing_rule: () =>
        Promise.resolve(
          ["alpha", "beta"].map(name => ({ name, synced: synced.has(name), read_only: false })),
        ),
      sync_rule_to_project: (args?: Record<string, unknown>) => {
        synced.add(String(args?.projectName));
        return Promise.resolve(null);
      },
    });
  });

  it("keeps the header out of sync while another project still needs an update", async () => {
    render(<Rules />);
    fireEvent.click(await screen.findByText("Team style"));

    const updates = await screen.findAllByRole("button", { name: /^Update$/ });
    expect(updates).toHaveLength(2);
    fireEvent.click(updates[0]);

    await waitFor(() => {
      expect(screen.getAllByRole("button", { name: /^Update$/ })).toHaveLength(1);
    });
    expect(screen.getByRole("button", { name: /Update all/ })).toBeInTheDocument();
    expect(screen.queryByText("In sync")).not.toBeInTheDocument();
  });

  it("shows in sync once every project is updated", async () => {
    render(<Rules />);
    fireEvent.click(await screen.findByText("Team style"));

    fireEvent.click(await screen.findByRole("button", { name: /Update all/ }));

    expect(await screen.findByText("In sync")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Update$/ })).not.toBeInTheDocument();
  });
});
