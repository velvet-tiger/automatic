import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { RebuildConfirmationModal } from "./RebuildConfirmationModal";

const preview = { project_name: "alpha", categories: [], changed: false };

describe("RebuildConfirmationModal", () => {
  it("lists the files rebuild could not read, with the reason", () => {
    const line = ".codex/agents/planner.toml: its description cannot be stored as frontmatter";
    render(
      <RebuildConfirmationModal
        preview={{ ...preview, refused_files: [line] }}
        busy={false}
        onConfirm={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText(line)).toBeInTheDocument();
    expect(screen.getByText(/They stay on disk/)).toBeInTheDocument();
  });

  it("says nothing about refused files when there are none", () => {
    render(
      <RebuildConfirmationModal preview={preview} busy={false} onConfirm={vi.fn()} onClose={vi.fn()} />,
    );

    expect(screen.queryByText(/They stay on disk/)).toBeNull();
  });
});
