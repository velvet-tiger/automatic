import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { GroupsPanel } from "./GroupsPanel";

function renderPanel(joinedGroups: string[]) {
  const onJoinGroup = vi.fn();
  render(
    <GroupsPanel
      projectKey="key"
      projectGroupMemberships={["team"]}
      joinedGroups={joinedGroups}
      allGroups={["team"]}
      loadingGroups={false}
      reloadGroups={vi.fn()}
      onAddToGroup={vi.fn()}
      onJoinGroup={onJoinGroup}
      onRemoveFromGroup={vi.fn()}
      onRemoveFromAllGroups={vi.fn()}
    />,
  );
  return onJoinGroup;
}

describe("GroupsPanel", () => {
  it("offers Join group for a listed checkout that has not joined", () => {
    const onJoinGroup = renderPanel([]);
    fireEvent.click(screen.getByText("Join group"));
    expect(onJoinGroup).toHaveBeenCalledWith("team", "key");
  });

  it("shows no join action once joined", () => {
    renderPanel(["team"]);
    expect(screen.queryByText("Join group")).not.toBeInTheDocument();
  });
});
