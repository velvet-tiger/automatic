/**
 * Render helper for Projects.tsx characterization tests.
 *
 * Mocks the React contexts and Tauri APIs so we can mount the page without
 * a running backend. Import `./tauriMock` BEFORE the component under test
 * to ensure `vi.mock` hoisting applies.
 */
import { render, type RenderResult } from "@testing-library/react";
import { vi } from "vitest";
import React from "react";

vi.mock("../contexts/ProfileContext", () => ({
  ProfileProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useCurrentUser: () => ({ profile: null, userId: "test-user", isLoaded: true }),
}));

vi.mock("../contexts/TaskLogContext", () => ({
  TaskLogProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useTaskLog: () => ({
    entries: [],
    isVisible: false,
    isManuallyOpened: false,
    log: () => "log-id",
    update: () => {},
    clear: () => {},
    dismiss: () => {},
    show: () => {},
  }),
}));

vi.mock("../lib/analytics", () => {
  // Permissive stub: every exported track* / identify / init is a no-op.
  const noop = () => {};
  return {
    track: noop,
    identify: noop,
    init: noop,
    trackNavigation: noop,
    trackSkillCreated: noop,
    trackSkillUpdated: noop,
    trackSkillDeleted: noop,
    trackSkillSynced: noop,
    trackAllSkillsSynced: noop,
    trackSkillInstalled: noop,
    trackProjectCreated: noop,
    trackProjectUpdated: noop,
    trackProjectDeleted: noop,
    trackProjectSynced: noop,
    trackProjectAgentAdded: noop,
    trackProjectAgentRemoved: noop,
    trackProjectSkillAdded: noop,
    trackProjectSkillRemoved: noop,
    trackProjectMcpServerAdded: noop,
    trackProjectMcpServerRemoved: noop,
    trackMcpServerCreated: noop,
    trackMcpServerUpdated: noop,
    trackMcpServerDeleted: noop,
    trackMemoryStored: noop,
    trackMemoryDeleted: noop,
    trackMemoryCleared: noop,
    trackSettingChanged: noop,
    trackUpdateChecked: noop,
    trackUpdateInstalled: noop,
  };
});

// Import after mocks so they take effect.
import Projects from "../pages/workspace/Projects";

export type ProjectsProps = React.ComponentProps<typeof Projects>;

export function renderProjects(props: ProjectsProps = {}): RenderResult {
  return render(<Projects {...props} />);
}
