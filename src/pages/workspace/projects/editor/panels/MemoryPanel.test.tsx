import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockInvoke, resetInvokeMock } from "../../../../../test/tauriMock";
import { MemoryPanel } from "./MemoryPanel";
import { emptyProject } from "../../helpers";
import type { MemoryRecord } from "../../../../../components/MemoryBrowser";

const claudeMemory = {
  memory_dir: "/Users/me/.claude/projects/-Users-me-code-app/memory",
  memory_md: null,
  topic_files: [{ name: "gotcha.md", content: "Watch out for the thing." }],
};

function projectAt(name: string, directory: string) {
  return { ...emptyProject(name), directory, agents: ["claude"] };
}

function renderPanel(projectKey: string, memories: MemoryRecord | null) {
  return (
    <MemoryPanel
      projectKey={projectKey}
      project={projectAt(projectKey, `/Users/me/code/${projectKey}`)}
      memories={memories}
      loadingMemories={false}
      reloadMemories={vi.fn().mockResolvedValue(undefined)}
      onError={vi.fn()}
    />
  );
}

/** The row header for a Claude memory file, found by its file name. */
async function fileRow(name: string): Promise<HTMLElement> {
  const label = await screen.findByText(name);
  const row = label.parentElement;
  if (!row) throw new Error(`no row for ${name}`);
  return row;
}

describe("MemoryPanel", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({ get_claude_memory: claudeMemory, store_memory: "Memory stored" });
  });

  it("drops the promotion confirmation when the project changes", async () => {
    const user = userEvent.setup();
    const view = render(renderPanel("alpha", {}));

    await user.click(within(await fileRow("gotcha.md")).getByRole("button", { name: /promote/i }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Promote" }));
    expect(await screen.findByRole("status")).toBeTruthy();

    view.rerender(renderPanel("beta", {}));

    await screen.findByText("gotcha.md");
    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  });

  it("shows no badges while memories are not loaded for the selected project", async () => {
    const stored = { "claude-memory/gotcha": { value: "Watch out for the thing.", timestamp: "2026-10-20T00:00:00Z", source: null } };
    const view = render(renderPanel("alpha", stored));
    expect(within(await fileRow("gotcha.md")).getByText("Promoted")).toBeTruthy();

    view.rerender(renderPanel("beta", null));

    const row = await fileRow("gotcha.md");
    await waitFor(() => expect(within(row).queryByText("Promoted")).toBeNull());
  });
});
