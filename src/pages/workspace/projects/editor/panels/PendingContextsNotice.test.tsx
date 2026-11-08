import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { PendingContextsNotice } from "./PendingContextsNotice";
import type { Project } from "../../types";

function project(overrides: Partial<Project> = {}): Project {
  return {
    name: "app",
    description: "",
    directory: "/tmp/app",
    skills: [],
    mcp_servers: [],
    providers: [],
    agents: [],
    created_at: "",
    updated_at: "",
    contexts: ["private"],
    pending_contexts: ["private"],
    ...overrides,
  } as Project;
}

describe("PendingContextsNotice", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({ attach_context: true, ignore_project_context: null });
  });

  it("names each pending context", () => {
    render(<PendingContextsNotice project={project()} selectedKey="key" reloadProject={vi.fn()} />);
    expect(screen.getByText('This project\'s shared settings include the context "private".')).toBeInTheDocument();
  });

  it("renders nothing once every pending context is ignored", () => {
    const { container } = render(
      <PendingContextsNotice
        project={project({ local_ignored_contexts: ["private"] })}
        selectedKey="key"
        reloadProject={vi.fn()}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("attaches to this project and reloads it", async () => {
    const reloadProject = vi.fn().mockResolvedValue(undefined);
    render(<PendingContextsNotice project={project()} selectedKey="key" reloadProject={reloadProject} />);
    fireEvent.click(screen.getByRole("button", { name: "Attach" }));
    await waitFor(() => expect(reloadProject).toHaveBeenCalledWith("key"));
    expect(invokeMock).toHaveBeenCalledWith(
      "attach_context",
      expect.objectContaining({ target: { type: "project", name: "key" }, slug: "private" }),
    );
  });

  it("ignores for this checkout", async () => {
    const reloadProject = vi.fn().mockResolvedValue(undefined);
    render(<PendingContextsNotice project={project()} selectedKey="key" reloadProject={reloadProject} />);
    fireEvent.click(screen.getByRole("button", { name: "Ignore" }));
    await waitFor(() => expect(reloadProject).toHaveBeenCalledWith("key"));
    expect(invokeMock).toHaveBeenCalledWith(
      "ignore_project_context",
      expect.objectContaining({ projectName: "key", slug: "private" }),
    );
  });

  it("shows a failed attach", async () => {
    mockInvoke({
      attach_context: () => {
        throw new Error("read only");
      },
    });
    render(<PendingContextsNotice project={project()} selectedKey="key" reloadProject={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Attach" }));
    expect(await screen.findByText(/Could not attach the context "private"/)).toBeInTheDocument();
  });
});
