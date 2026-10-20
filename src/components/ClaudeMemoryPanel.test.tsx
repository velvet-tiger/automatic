import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invokeMock, mockInvoke, resetInvokeMock, type InvokeArgs } from "../test/tauriMock";
import { ClaudeMemoryPanel, promotionStatus } from "./ClaudeMemoryPanel";

const claudeMemory = {
  memory_dir: "/Users/me/.claude/projects/-Users-me-code-app/memory",
  memory_md: "# Index\n- [Gotcha](gotcha.md)\n",
  topic_files: [{ name: "gotcha.md", content: "  Watch out for the thing.\n" }],
};

function storeCalls(): InvokeArgs[] {
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "store_memory")
    .map(([, args]) => args as InvokeArgs);
}

function renderPanel(storedMemories: Record<string, { value: string }> = {}) {
  const onPromoted = vi.fn();
  render(
    <ClaudeMemoryPanel
      projectKey="local-key-1"
      projectDirectory="/Users/me/code/app"
      storedMemories={storedMemories}
      onPromoted={onPromoted}
    />,
  );
  return { onPromoted };
}

/** The row header for a Claude memory file, found by its file name. */
async function fileRow(name: string): Promise<HTMLElement> {
  const label = await screen.findByText(name);
  const row = label.parentElement;
  if (!row) throw new Error(`no row for ${name}`);
  return row;
}

describe("promotionStatus", () => {
  it("is none when the key is not stored", () => {
    expect(promotionStatus({}, "claude-memory/a", "x")).toBe("none");
  });

  it("is promoted when the stored value matches the trimmed content", () => {
    expect(promotionStatus({ k: { value: "body" } }, "k", "  body\n")).toBe("promoted");
  });

  it("is changed when the stored value differs", () => {
    expect(promotionStatus({ k: { value: "old" } }, "k", "new")).toBe("changed");
  });
});

describe("ClaudeMemoryPanel", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({
      get_claude_memory: claudeMemory,
      store_memory: "Memory stored",
    });
  });

  it("stores a topic file and confirms the promotion", async () => {
    const user = userEvent.setup();
    const { onPromoted } = renderPanel();

    const row = await fileRow("gotcha.md");
    await user.click(within(row).getByRole("button", { name: /promote/i }));

    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Promote" }));

    await waitFor(() => expect(storeCalls()).toHaveLength(1));
    expect(storeCalls()[0]).toEqual({
      project: "local-key-1",
      key: "claude-memory/gotcha",
      value: "Watch out for the thing.",
      source: "claude-auto-memory",
    });
    expect(onPromoted).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();

    const status = await screen.findByRole("status");
    expect(status.textContent).toContain("claude-memory/gotcha");
  });

  it("marks a file that is already in Automatic memory", async () => {
    renderPanel({ "claude-memory/gotcha": { value: "Watch out for the thing." } });

    const row = await fileRow("gotcha.md");
    expect(within(row).getByText("Promoted")).toBeTruthy();
  });

  it("marks a file that changed since it was promoted", async () => {
    renderPanel({ "claude-memory/gotcha": { value: "An older version." } });

    const row = await fileRow("gotcha.md");
    expect(within(row).getByText("Changed since promoted")).toBeTruthy();
  });

  it("warns before replacing an existing memory", async () => {
    const user = userEvent.setup();
    renderPanel({ "claude-memory/MEMORY": { value: "old index" } });

    const row = await fileRow("MEMORY.md");
    await user.click(within(row).getByRole("button", { name: /promote/i }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/already exists/i)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Replace" })).toBeTruthy();
  });

  it("shows a store failure in the dialog", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_claude_memory") return Promise.resolve(claudeMemory);
      if (cmd === "store_memory") return Promise.reject("Invalid project name");
      return Promise.resolve(undefined);
    });
    const { onPromoted } = renderPanel();

    const row = await fileRow("gotcha.md");
    await user.click(within(row).getByRole("button", { name: /promote/i }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Promote" }));

    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toContain("Invalid project name");
    expect(onPromoted).not.toHaveBeenCalled();
  });

  it("shows a read failure instead of an empty panel", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "get_claude_memory"
        ? Promise.reject("Project 'app' not found")
        : Promise.resolve(undefined),
    );
    renderPanel();

    expect(await screen.findByText(/Failed to read Claude auto-memory/)).toBeTruthy();
  });
});
