import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { DriftDiffModal } from "./DriftDiffModal";

describe("DriftDiffModal", () => {
  it("tells the user a refused file is theirs to delete and offers no overwrite", () => {
    render(
      <DriftDiffModal
        file={{ path: ".codex/agents/planner.toml", reason: "refused", actual: "name = 1\n" }}
        agentLabel="Codex CLI"
        projectKey="alpha"
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText("Automatic cannot read this file")).toBeInTheDocument();
    expect(screen.getByText(/Delete it yourself/)).toBeInTheDocument();
    expect(screen.queryByText(/Overwrite/)).toBeNull();
  });
});
