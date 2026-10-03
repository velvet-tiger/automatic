import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { GroupProfilesSection } from "./GroupProfilesSection";

function renderSection(profiles: string[]) {
  const onChanged = vi.fn();
  render(<GroupProfilesSection groupName="team" profiles={profiles} onChanged={onChanged} />);
  return onChanged;
}

describe("GroupProfilesSection", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({ get_project_profiles: ["baseline", "extra"] });
  });

  it("attaches a profile to the group", async () => {
    const onChanged = renderSection(["baseline"]);
    fireEvent.click(screen.getByText("Attach profile"));
    fireEvent.click(await screen.findByText("extra"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("attach_profile_to_group", { groupName: "team", profileName: "extra" });
      expect(onChanged).toHaveBeenCalledTimes(1);
    });
  });

  it("detaches a profile from the group", async () => {
    const onChanged = renderSection(["baseline"]);
    fireEvent.click(screen.getByLabelText("Detach baseline from the group"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("detach_profile_from_group", { groupName: "team", profileName: "baseline" });
      expect(onChanged).toHaveBeenCalledTimes(1);
    });
  });

  it("shows the backend's error and does not report a change", async () => {
    mockInvoke({
      get_project_profiles: ["baseline"],
      detach_profile_from_group: () => { throw new Error("group is locked"); },
    });
    const onChanged = renderSection(["baseline"]);
    fireEvent.click(screen.getByLabelText("Detach baseline from the group"));
    expect(await screen.findByText(/Failed to detach profile "baseline".*group is locked/)).toBeInTheDocument();
    expect(onChanged).not.toHaveBeenCalled();
  });

  it("flags a profile the library no longer has", async () => {
    renderSection(["gone"]);
    expect(await screen.findByText("Missing from library")).toBeInTheDocument();
  });

  it("explains group profiles when none are attached", () => {
    renderSection([]);
    expect(screen.getByText(/Every project in the group receives the profiles you attach here/)).toBeInTheDocument();
  });
});
