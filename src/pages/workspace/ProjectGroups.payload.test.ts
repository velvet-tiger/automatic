import { describe, it, expect } from "vitest";
import "../../test/tauriMock";
import { groupSavePayload } from "./ProjectGroups";

describe("groupSavePayload", () => {
  it("leaves the edit-across switch out, so the backend keeps the stored value", () => {
    const sent = JSON.parse(
      groupSavePayload({
        name: "Team",
        description: "d",
        projects: ["a"],
        agents_edit_across_group: true,
        created_at: "",
        updated_at: "",
      }),
    );
    expect(sent).toEqual({ name: "Team", description: "d", projects: ["a"], created_at: "", updated_at: "" });
  });
});
