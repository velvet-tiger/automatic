import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { PendingRegistrationNotice } from "./PendingRegistrationNotice";

function renderNotice() {
  const props = {
    projectName: "repo",
    selectedKey: "key",
    reloadProject: vi.fn().mockResolvedValue(undefined),
    onRemove: vi.fn().mockResolvedValue(undefined),
  };
  render(<PendingRegistrationNotice {...props} />);
  return props;
}

describe("PendingRegistrationNotice", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({ confirm_project_registration: null });
  });

  it("explains what waits for the user", () => {
    renderNotice();
    expect(screen.getByText("An agent added this project.")).toBeInTheDocument();
  });

  it("keeps the project and reloads it", async () => {
    const props = renderNotice();
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    await waitFor(() => expect(props.reloadProject).toHaveBeenCalledWith("key"));
    expect(invokeMock).toHaveBeenCalledWith(
      "confirm_project_registration",
      expect.objectContaining({ projectName: "key" }),
    );
    expect(props.onRemove).not.toHaveBeenCalled();
  });

  it("hands removal to the normal project removal", async () => {
    const props = renderNotice();
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(props.onRemove).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("confirm_project_registration", expect.anything());
  });

  it("shows a failed keep", async () => {
    mockInvoke({
      confirm_project_registration: () => {
        throw new Error("no pointer");
      },
    });
    renderNotice();
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(await screen.findByText(/Could not keep "repo"/)).toBeInTheDocument();
  });
});
