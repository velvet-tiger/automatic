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

function renderPanel(storedMemories: Record<string, { value: string }> | undefined = {}) {
  const onPromoted = vi.fn();
  const view = render(
    <ClaudeMemoryPanel
      projectKey="local-key-1"
      projectDirectory="/Users/me/code/app"
      storedMemories={storedMemories}
      onPromoted={onPromoted}
    />,
  );
  return { onPromoted, view };
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

  it("is differs when the stored value is not the file content", () => {
    expect(promotionStatus({ k: { value: "old" } }, "k", "new")).toBe("differs");
  });

  it("follows a recorded promotion over the suggested key", () => {
    const stored = { edited: { value: "user edit" }, k: { value: "other" } };
    const promotion = { key: "edited", content: "file body" };
    expect(promotionStatus(stored, "k", "file body", promotion)).toBe("promoted");
    expect(promotionStatus(stored, "k", "file body v2", promotion)).toBe("changed");
    expect(promotionStatus({}, "k", "file body", promotion)).toBe("none");
  });

  it("ignores inherited properties", () => {
    expect(promotionStatus({}, "constructor", "x")).toBe("none");
    expect(promotionStatus({}, "__proto__", "x")).toBe("none");
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
    expect(status.textContent).toContain("Agent Memory");
  });

  it("marks a file that is already in Automatic memory", async () => {
    renderPanel({ "claude-memory/gotcha": { value: "Watch out for the thing." } });

    const row = await fileRow("gotcha.md");
    expect(within(row).getByText("Promoted")).toBeTruthy();
  });

  it("marks a file whose suggested key holds another value", async () => {
    renderPanel({ "claude-memory/gotcha": { value: "An older version." } });

    const row = await fileRow("gotcha.md");
    expect(within(row).getByText("Differs from Automatic memory")).toBeTruthy();
  });

  it("marks nothing while memories are not known for the project", async () => {
    const user = userEvent.setup();
    renderPanel(undefined);

    const row = await fileRow("MEMORY.md");
    expect(within(row).queryByText("Promoted")).toBeNull();
    await user.click(within(row).getByRole("button", { name: /promote/i }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByText(/already exists/i)).toBeNull();
  });

  it("shows the file as promoted after the user edits the key and value", async () => {
    const user = userEvent.setup();
    const { view } = renderPanel();

    const row = await fileRow("gotcha.md");
    await user.click(within(row).getByRole("button", { name: /promote/i }));
    const dialog = await screen.findByRole("dialog");
    const [keyInput, valueInput] = [
      within(dialog).getByRole("textbox", { name: /key/i }),
      within(dialog).getByRole("textbox", { name: /value/i }),
    ];
    await user.clear(keyInput);
    await user.type(keyInput, "gotchas/thing");
    await user.clear(valueInput);
    await user.type(valueInput, "Edited summary");
    await user.click(within(dialog).getByRole("button", { name: "Promote" }));
    await waitFor(() => expect(storeCalls()).toHaveLength(1));
    expect(storeCalls()[0]).toMatchObject({ key: "gotchas/thing", value: "Edited summary" });

    // The parent reloads Automatic's memories and passes them back in.
    view.rerender(
      <ClaudeMemoryPanel
        projectKey="local-key-1"
        projectDirectory="/Users/me/code/app"
        storedMemories={{ "gotchas/thing": { value: "Edited summary" } }}
        onPromoted={vi.fn()}
      />,
    );

    const rowAfter = await fileRow("gotcha.md");
    expect(within(rowAfter).getByText("Promoted")).toBeTruthy();
    expect(within(rowAfter).queryByText(/Differs|Changed/)).toBeNull();
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
