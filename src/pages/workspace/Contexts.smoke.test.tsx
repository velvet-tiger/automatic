import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock, type InvokeArgs } from "../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import Contexts from "./Contexts";

const PAYMENTS = {
  slug: "payments",
  display_name: "Payments",
  description: "Payment flows",
  location: "local",
  sources: [
    { id: "pages", display_name: "", description: "", kind: "documentation" },
    { id: "adr", display_name: "Decisions", description: "", kind: "local", config: { path: "/tmp/adr" } },
  ],
  created_at: "t",
  updated_at: "t",
};

function calls(cmd: string): InvokeArgs[] {
  return invokeMock.mock.calls.filter(([c]) => c === cmd).map(([, args]) => args as InvokeArgs);
}

describe("Contexts", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({ list_contexts: [] });
  });

  it("invites you to create the first context", async () => {
    render(<Contexts />);
    expect(await screen.findByText("Give your agents some background")).toBeInTheDocument();
  });

  it("creates a context from just a name, with pages ready", async () => {
    mockInvoke({
      list_contexts: [],
      save_context: (args: InvokeArgs) => args?.context,
      read_context: { ...PAYMENTS, slug: "team-docs", display_name: "Team docs", sources: [PAYMENTS.sources[0]] },
      list_context_documentation_pages: [],
      get_context_references: { projects: [], groups: [] },
    });
    render(<Contexts />);
    fireEvent.click((await screen.findAllByText("New context"))[0]!);
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Team docs" } });
    fireEvent.click(screen.getByText("Create context"));
    await waitFor(() => {
      expect(calls("save_context")[0]).toEqual({
        context: expect.objectContaining({
          slug: "team-docs",
          display_name: "Team docs",
          location: "local",
          sources: [expect.objectContaining({ id: "pages", kind: "documentation" })],
        }),
      });
    });
    expect(await screen.findByText("Write your first page")).toBeInTheDocument();
  });

  it("shows pages as a folder tree and linked material by name", async () => {
    mockInvoke({
      list_contexts: [PAYMENTS],
      read_context: PAYMENTS,
      list_context_documentation_pages: ["index.md", "guides/setup.md"],
      get_context_references: { projects: ["app"], groups: [] },
    });
    render(<Contexts />);
    fireEvent.click(await screen.findByText("Payments"));
    expect(await screen.findByText("Guides")).toBeInTheDocument();
    expect(screen.getByText("Setup")).toBeInTheDocument();
    expect(screen.getByText("Index")).toBeInTheDocument();
    expect(screen.getByText("Decisions")).toBeInTheDocument();
    expect(screen.getByText("/tmp/adr")).toBeInTheDocument();
    expect(screen.queryByText("Source id")).not.toBeInTheDocument();
    expect(await screen.findByText("app")).toBeInTheDocument();
  });

  it("creates a page inside a folder from its title", async () => {
    mockInvoke({
      list_contexts: [PAYMENTS],
      read_context: PAYMENTS,
      list_context_documentation_pages: ["guides/setup.md"],
      read_context_documentation_page: "---\ntype: page\n---\n\n# Body\n",
      get_context_references: { projects: [], groups: [] },
    });
    render(<Contexts />);
    fireEvent.click(await screen.findByText("Payments"));
    fireEvent.click(await screen.findByLabelText("New page in Guides"));
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Deploying" } });
    fireEvent.click(screen.getByText("Create page"));
    await waitFor(() => {
      expect(calls("write_context_documentation_page")[0]).toEqual(
        expect.objectContaining({ slug: "payments", sourceId: "pages", path: "guides/deploying.md" }),
      );
    });
  });

  it("moves a page to another folder", async () => {
    mockInvoke({
      list_contexts: [PAYMENTS],
      read_context: PAYMENTS,
      list_context_documentation_pages: ["guides/setup.md", "decisions/one.md"],
      read_context_documentation_page: "# Setup\n",
      get_context_references: { projects: [], groups: [] },
    });
    render(<Contexts />);
    fireEvent.click(await screen.findByText("Payments"));
    fireEvent.click(await screen.findByText("Setup"));
    fireEvent.click(await screen.findByText("Move"));
    fireEvent.change(screen.getByLabelText("Destination folder"), { target: { value: "decisions" } });
    fireEvent.click(screen.getByText("Move page"));
    await waitFor(() => {
      expect(calls("move_context_documentation_page")[0]).toEqual({
        slug: "payments",
        sourceId: "pages",
        from: "guides/setup.md",
        to: "decisions/setup.md",
      });
    });
  });
});
