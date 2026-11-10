import { describe, it, expect, beforeEach, vi } from "vitest";
import { mockInvoke, resetInvokeMock, invokeMock } from "../../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { GroupEditAcrossSection } from "./GroupEditAcrossSection";

describe("GroupEditAcrossSection", () => {
  beforeEach(() => {
    resetInvokeMock();
    mockInvoke({ set_group_agents_edit_across_group: null });
  });

  it("turns the switch on at once and reloads", async () => {
    const onChanged = vi.fn();
    render(<GroupEditAcrossSection groupName="Team" enabled={false} onChanged={onChanged} />);
    const toggle = screen.getByRole("switch", { name: "Let agents edit across this group" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    fireEvent.click(toggle);
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(invokeMock).toHaveBeenCalledWith("set_group_agents_edit_across_group", { groupName: "Team", enabled: true });
  });

  it("turns it off", async () => {
    render(<GroupEditAcrossSection groupName="Team" enabled onChanged={vi.fn()} />);
    fireEvent.click(screen.getByRole("switch"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_group_agents_edit_across_group", { groupName: "Team", enabled: false }),
    );
  });

  it("shows a failure without reloading", async () => {
    mockInvoke({
      set_group_agents_edit_across_group: () => {
        throw "group not found";
      },
    });
    const onChanged = vi.fn();
    render(<GroupEditAcrossSection groupName="Team" enabled={false} onChanged={onChanged} />);
    fireEvent.click(screen.getByRole("switch"));
    expect(await screen.findByText(/group not found/)).toBeInTheDocument();
    expect(onChanged).not.toHaveBeenCalled();
  });
});
