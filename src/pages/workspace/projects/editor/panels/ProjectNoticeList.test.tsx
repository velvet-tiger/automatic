import { describe, it, expect, beforeEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { ProjectNoticeList } from "./ProjectNoticeList";
import type { ProjectNotice } from "../../types";

const problem: ProjectNotice = {
  id: "missing_directory",
  severity: "problem",
  count: 1,
  source: { kind: "missing_directory", directory: "/tmp/app" },
};
const drift: ProjectNotice = {
  id: "drift",
  severity: "warning",
  count: 1,
  source: { kind: "drift", report: { drifted: true, agents: [] } },
};
const profiles: ProjectNotice = {
  id: "pending_profiles",
  severity: "warning",
  count: 2,
  source: { kind: "pending_profiles", names: ["a", "b"] },
};
const silent: ProjectNotice = {
  id: "silent_mode",
  severity: "notice",
  count: 1,
  source: { kind: "silent_mode", directory: "/tmp/app" },
};

const renderRow = (n: ProjectNotice) => <p>row {n.id}</p>;

describe("ProjectNoticeList", () => {
  beforeEach(() => localStorage.clear());

  it("renders nothing with no notices", () => {
    const { container } = render(<ProjectNoticeList projectKey="app" notices={[]} renderNotice={renderRow} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows counts by severity and every row", () => {
    render(<ProjectNoticeList projectKey="app" notices={[problem, drift, profiles, silent]} renderNotice={renderRow} />);
    expect(screen.getByText("Needs attention")).toBeInTheDocument();
    expect(screen.getByText("1 problem")).toBeInTheDocument();
    expect(screen.getByText("3 warnings")).toBeInTheDocument();
    expect(screen.getByText("1 notice")).toBeInTheDocument();
    expect(screen.getByText("row pending_profiles")).toBeInTheDocument();
  });

  it("is headed Notices when there is nothing to act on", () => {
    render(<ProjectNoticeList projectKey="app" notices={[silent]} renderNotice={renderRow} />);
    expect(screen.getByText("Notices")).toBeInTheDocument();
    expect(screen.queryByText("Needs attention")).not.toBeInTheDocument();
  });

  it("hides the rows but keeps the counts, and remembers it per project", () => {
    const { unmount } = render(<ProjectNoticeList projectKey="app" notices={[problem, drift]} renderNotice={renderRow} />);
    fireEvent.click(screen.getByRole("button", { name: /Needs attention/ }));
    expect(screen.queryByText("row drift")).not.toBeInTheDocument();
    expect(screen.getByText("1 problem")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Needs attention/ })).toHaveAttribute("aria-expanded", "false");
    unmount();

    render(<ProjectNoticeList projectKey="app" notices={[problem, drift]} renderNotice={renderRow} />);
    expect(screen.queryByText("row drift")).not.toBeInTheDocument();
  });

  it("opens again when a new problem appears", () => {
    const { rerender } = render(<ProjectNoticeList projectKey="app" notices={[drift]} renderNotice={renderRow} />);
    fireEvent.click(screen.getByRole("button", { name: /Needs attention/ }));
    expect(screen.queryByText("row drift")).not.toBeInTheDocument();

    rerender(<ProjectNoticeList projectKey="app" notices={[problem, drift]} renderNotice={renderRow} />);
    expect(screen.getByText("row missing_directory")).toBeInTheDocument();
    expect(localStorage.getItem("automatic.project_notices.hidden.app")).toBeNull();
  });

  it("keeps each project's hidden state apart", () => {
    const { rerender } = render(<ProjectNoticeList projectKey="app" notices={[drift]} renderNotice={renderRow} />);
    fireEvent.click(screen.getByRole("button", { name: /Needs attention/ }));
    rerender(<ProjectNoticeList projectKey="other" notices={[drift]} renderNotice={renderRow} />);
    expect(screen.getByText("row drift")).toBeInTheDocument();
  });
});
