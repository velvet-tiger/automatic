# Projects.tsx — Phase 2 Refactor Plan (test-harness-first)

> Execution plan for a **fresh agent session**. Self-contained: you should not need
> prior conversation context. Read "Background" then execute the phases in order.
> Each step ends with a concrete **Verify** gate — do not proceed past a failing gate.

## Background — where things stand

`src/pages/workspace/Projects.tsx` (~7,530 lines as of commit `d641a43`) is a single
React component that renders **two screens** behind one conditional:

- An **early return** — `if (!selectedName && !isCreating) return <ProjectsOverview … />`
  — is the **projects list** screen.
- The **main return** is the **single-project editor** (plus the new-project wizard,
  since `isCreating` also falls through here).

Both branches share one closure with **131 `useState`** and **16 `useEffect`**, but
nearly all of that state + effects belong to the editor, not the list. This fusion is
the architectural problem and the reason the file is huge.

**Phase 1 (DONE, committed `d641a43`)** extracted all standalone module-scope code into
`src/pages/workspace/projects/`:

```
projects/
  types.ts  helpers.tsx  diff.ts  EditorIcon.tsx       ← shared utilities
  modals/      DriftDiffModal, InstructionConflictModal, SwitchToUnifiedModal,
               RebuildConfirmationModal, ApplyProjectTemplateModal
  overview/    ProjectsOverview.tsx  (list screen; ProjectStatusBadge/ProjectCard/
               ProjectsHealthBar are private inside it)
  editor/      ActivityFeed.tsx, SummaryMetricCard.tsx  (editor Summary-tab pieces)
  tools/       ProjectToolsTab.tsx  (ProjectToolRow private inside it)
  SkillAddButton.tsx  McpAddButton.tsx
```

**Phase 2 (THIS PLAN)** splits the two screens: `Projects.tsx` becomes a thin **router**;
the editor moves into `projects/editor/ProjectEditor.tsx`, mounting only when a project
is selected or being created.

## Why test-harness-first

Unlike Phase 1 (mechanical, compiler-proved), the carve-out is **behavioral**: it
rewrites the selection and creation control flow and replaces shared-closure list
refresh with explicit callbacks. `tsc` confirms it compiles but **cannot** catch a
stale list after an edit, a broken create-from-template flow, the drift modal rendering
on the wrong screen, or effects firing at the wrong time. There is **no frontend test
suite today**, and the Tauri GUI cannot be driven in CI. So: build a test harness and
pin current behavior with characterization tests FIRST, then refactor against a green
suite.

---

## Phase 2A — Test harness (vitest + React Testing Library)

Goal: `npm test` mounts React components in jsdom with Tauri mocked.

### Step A1 — install dev dependencies
- **Action:** `npm i -D vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom`
- **Verify:** `npx vitest --version` prints a version.
- **Done when:** deps appear in `package.json` devDependencies.

### Step A2 — vitest config
- **Action:** add a `test` block to `vite.config.ts` (it already imports the React
  plugin). Use:
  ```ts
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
  }
  ```
  (If `vite.config.ts` typing complains, add `/// <reference types="vitest/config" />`
  at the top, or move the block to a new `vitest.config.ts`.)
- **Verify:** `npx vitest run` starts (no config error) even with zero tests.
- **Done when:** vitest runs and reports "no test files".

### Step A3 — global test setup + Tauri mock helper
- **Action:** create `src/test/setup.ts`:
  ```ts
  import "@testing-library/jest-dom";
  ```
  Create `src/test/tauriMock.ts` exporting a configurable invoke mock:
  ```ts
  import { vi } from "vitest";
  export type InvokeHandler = (cmd: string, args?: any) => unknown;
  export const invokeMock = vi.fn();
  vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: any[]) => (invokeMock as any)(...a) }));
  vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn().mockResolvedValue(true) }));
  // Helper: route invoke by command name to canned responses.
  export function mockInvoke(routes: Record<string, unknown | ((args: any) => unknown)>) {
    invokeMock.mockImplementation((cmd: string, args: any) => {
      const r = routes[cmd];
      if (r === undefined) return Promise.resolve(undefined);
      return Promise.resolve(typeof r === "function" ? (r as any)(args) : r);
    });
  }
  ```
  NOTE: `vi.mock` is hoisted; import `tauriMock` before the component under test.
- **Verify:** a trivial test importing the helper runs green.
- **Done when:** `src/test/setup.ts` and `src/test/tauriMock.ts` exist.

### Step A4 — render helper that supplies contexts
- **Action:** create `src/test/renderProjects.tsx` that wraps a component in the real
  providers (`ProfileProvider`/`TaskLogProvider` from `src/contexts/`) OR mocks the
  hooks `useCurrentUser`/`useTaskLog` if the providers need network. Prefer mocking:
  ```ts
  vi.mock("../contexts/ProfileContext", () => ({ useCurrentUser: () => ({ userId: "test-user" }) }));
  vi.mock("../contexts/TaskLogContext", () => ({ useTaskLog: () => ({ log: () => "id", update: () => {} }) }));
  ```
  (Adjust the returned shapes to match the real hooks — read the context files first.)
- **Verify:** importing the helper compiles.
- **Done when:** helper exists and exports `renderProjects(props?)`.

### Step A5 — wire scripts + smoke test
- **Action:** add to `package.json`: `"test": "vitest run"`, `"test:watch": "vitest"`.
  Optionally add `npm test` to the `check` target in `Makefile`.
  Write `src/pages/workspace/Projects.smoke.test.tsx` that mocks `get_projects`→`[]`
  and asserts the component renders without throwing.
- **Verify:** `npm test` → 1 passing test.
- **Done when:** `npm test` is green and runs in CI-friendly mode.

---

## Phase 2B — Characterization tests (pin CURRENT behavior)

Goal: lock the behaviors the carve-out must preserve. **These must pass against the
current (pre-carve-out) `Projects.tsx`.** Keep each test small; mock only the commands
that test needs (see Appendix C for the command list).

Write `src/pages/workspace/Projects.behavior.test.tsx` with one test per behavior:

- **B1 list renders:** `get_projects`→`["alpha","beta"]`, `read_project`→canned Project;
  assert both names appear.
- **B2 select opens editor:** click a project card → asserts `read_project` called with
  that name AND the editor chrome appears (e.g. the "Projects" back button / project
  title). 
- **B3 back returns to list:** from the editor, click back → list visible again.
- **B4 create blank:** trigger create (the "+"/New Project affordance) → wizard step 1
  visible.
- **B5 create-from-template:** dispatch the path that sets `initialCreateWithTemplate`
  (or the create-project window event) with templates mocked → wizard seeded.
- **B6 save refreshes overview:** open a project, save (`save_project` mocked) → assert
  the overview detail/drift for that project updates (go back, check no drift badge).
- **B7 delete:** delete a project (`delete_project` mocked, `ask`→true) → removed from
  list.

Document any behavior you CANNOT easily test (note it in the test file as a TODO) so the
manual GUI checklist (Appendix D) covers the gap.

- **Verify:** `npm test` → all B-tests green on current code.
- **Done when:** committed as a checkpoint: `test(projects): characterization tests for list/editor behavior`.

---

## Phase 2C — Folder realignment (mechanical, safe)

Goal: group files by screen before introducing `ProjectEditor`. Pure file moves +
import-path updates; compiler-verified, no behavior change (same kind as Phase 1).

### Step C1 — move editor-only leaf files into `editor/`
- **Move:** `SkillAddButton.tsx`, `McpAddButton.tsx`, the `tools/` folder, and the 3
  **editor-only** modals (`SwitchToUnifiedModal`, `RebuildConfirmationModal`,
  `ApplyProjectTemplateModal`) → under `projects/editor/`.
- **Keep in `modals/`:** `DriftDiffModal`, `InstructionConflictModal` (rendered by BOTH
  screens — see Appendix B).
- **Action:** `git mv` each; update import specifiers in `Projects.tsx` and inside any
  moved file whose relative imports change depth (e.g. a file going from
  `projects/X.tsx` to `projects/editor/X.tsx` changes `./types`→`../types`,
  `../../../components`→`../../../../components`).
- **Verify:** `npm run lint` clean; `npm test` green.
- **Done when:** committed: `refactor(projects): group editor-only files under editor/`.

---

## Phase 2D — Carve out `ProjectEditor` (the behavioral split)

Do each step, then run `npm run lint` AND `npm test`. Commit after D8.

### Step D1 — scaffold ProjectEditor from a copy
- **Action:** `cp src/pages/workspace/Projects.tsx src/pages/workspace/projects/editor/ProjectEditor.tsx`
- Fix import paths (file is 2 dirs deeper than `Projects.tsx`). Apply, in order:
  ```
  s|"\./projects/editor/|"./|g
  s|"\./projects/modals/|"../modals/|g      # adjust if C1 moved some modals to editor/
  s|"\./projects/tools/|"./tools/|g          # after C1 tools/ is under editor/
  s|"\./projects/overview/|"../overview/|g
  s|"\./projects/|"../|g
  s|"\.\./\.\./|"../../../../|g              # components/contexts/lib/plugins
  ```
  (Re-derive these against the actual import lines after C1 — the moves change targets.)
- Remove the `ProjectsOverview` import (editor does not render it).
- **Verify:** paths resolve once D2 is done (this step alone won't compile).

### Step D2 — signature + props; list state → props
- Replace `interface ProjectsProps {…}` with `interface ProjectEditorProps` and the
  `export default function Projects(…)` signature with
  `export function ProjectEditor(props: ProjectEditorProps)`. Use the **exact interface
  in Appendix A**.
- **Delete** these `useState` declarations from the editor (they are now props — same
  names, so the body is unchanged): `projects`, `projectsLoading`, `selectedName`,
  `isCreating`, `syncAllStatus`, `driftByProject`, `projectDetailsMap`.
  - `selectedName`/`setSelectedName`, `isCreating`/`setIsCreating`,
    `setProjectDetailsMap`, `setDriftByProject` come from props.
  - `projects`, `projectsLoading`, `syncAllStatus` are NOT used by the editor — confirm
    via `tsc` (if it complains, add as a prop).
- **Verify:** `tsc` errors now point only at list-only fns/effects (next step).

### Step D3 — remove list-only effects/functions from the editor
Delete these (they live in the router — Appendix B):
- `applyStoredOrder`, `loadProjects`
- `handleSyncAll`
- the "Background drift check for all projects" effect (deps `[projects]`)
- the "create-project" window-listener effect
- the "project-removed" window-listener effect
- the `resetKey` effect
- the `initialProject`-navigation effect
- the legacy localStorage-migration effect
- **Verify:** `tsc` errors now only about `loadProjects` calls + the overview return.

### Step D4 — remove the overview early-return
- Delete the `if (!selectedName && !isCreating) { return (<>…ProjectsOverview…</>); }`
  block from the editor.
- **Verify:** `tsc`.

### Step D5 — selection load on mount (SEAM 1)
- The editor must load its project when `selectedName` changes. Add near the top of the
  body:
  ```ts
  useEffect(() => {
    if (selectedName && !isCreating) void selectProject(selectedName);
  }, [selectedName]);
  ```
  This replaces the eager `selectProject(name)` previously called from the overview
  click and the `initialProject` effect. `selectProject` STAYS in the editor.
- **Verify:** `tsc`.

### Step D6 — create flow on mount (SEAM 2 — the tricky one)
- In the editor, `startCreate` currently both flips `isCreating` and inits the wizard.
  After the split the ROUTER flips `isCreating` (to mount the editor); the editor runs
  the wizard init on mount. Refactor:
  - Remove any `setIsCreating(true)` inside the editor's `startCreate` (router owns it).
  - Add a mount effect:
    ```ts
    useEffect(() => {
      if (isCreating) void startCreate(createFromTemplates ? { fromTemplates: createFromTemplates } : undefined);
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);
    ```
  - After consuming `createFromTemplates`, call `onCreateFromTemplatesConsumed?.()`.
- Keep the editor's `initialProjectTab` effect (it runs after selection; harmless).
- **Verify:** `tsc`; B4/B5 tests will validate this in D9.

### Step D7 — list refresh callbacks + back button (SEAM 3)
- Replace the editor's `loadProjects()` calls (in `handleSave`, `handleRemove`, project
  create, rename) with `reloadProjects()` (the prop).
- Replace the editor's `handleBackToOverview` body with a call to `onBack()` (remove the
  local routing mutations; the router handles them). Update the back button
  `onClick={onBack}`.
- Leave `setProjectDetailsMap(...)` / `setDriftByProject(...)` calls as-is — they now
  use the prop setters and keep the overview in sync.
- **Verify:** `tsc` clean for ProjectEditor.tsx.

### Step D8 — reduce `Projects.tsx` to the router
Rewrite `Projects.tsx` to keep ONLY:
- **State:** `projects`, `projectsLoading`, `selectedName`, `isCreating`,
  `projectDetailsMap`, `driftByProject`, `syncAllStatus`, plus a small
  `createFromTemplates` state for SEAM 2 seeding.
- **Functions/effects (copy verbatim from the pre-carve-out file):** `applyStoredOrder`,
  `loadProjects`, `handleSyncAll`, the background-drift-all effect, the legacy-migration
  effect, the `create-project` listener (→ `setIsCreating(true)`), the `project-removed`
  listener (→ clear selection if it matches + `loadProjects()`), the `resetKey` effect
  (→ clear selection), the `initialProject` effect (→ `setSelectedName(initialProject)`),
  and an effect to resolve `initialCreateWithTemplate` → set `createFromTemplates` +
  `setIsCreating(true)` (needs `availableProjectTemplates`; either load a lightweight
  template list in the router via `get_project_templates`, or pass the raw name through
  and resolve inside the editor — pick one and note it).
- **Handlers:** `handleSyncAll`; `handleBackToOverview` (clears `selectedName`,
  `isCreating`, `createFromTemplates`, removes `LAST_PROJECT_KEY`).
- **Render:**
  ```tsx
  if (!selectedName && !isCreating) {
    return (<><ProjectsOverview … onSelect={setSelectedName} onCreate={() => setIsCreating(true)} … />
             {/* shared modals: DriftDiffModal, InstructionConflictModal if list can open them */}</>);
  }
  return (<ProjectEditor
            selectedName={selectedName} setSelectedName={setSelectedName}
            isCreating={isCreating} setIsCreating={setIsCreating}
            reloadProjects={loadProjects}
            setProjectDetailsMap={setProjectDetailsMap} setDriftByProject={setDriftByProject}
            onBack={handleBackToOverview}
            createFromTemplates={createFromTemplates} onCreateFromTemplatesConsumed={() => setCreateFromTemplates(null)}
            initialProjectTab={initialProjectTab} onInitialProjectTabConsumed={onInitialProjectTabConsumed}
            onNavigateToSkill={onNavigateToSkill} … (all navigate* passthroughs) />);
  ```
- Keep `Projects.tsx`'s own props (`ProjectsProps`) — it is still the page entry point
  used by `App.tsx`; do not change its public signature.
- Trim now-unused imports (tsc `noUnusedLocals` will list them).
- **Verify:** `npm run lint` clean.

### Step D9 — full verification
- **Verify:** `npm run lint` clean; `npm test` (all B-tests still green); `npm run build`
  succeeds; `make check` (adds `cargo check`).
- Run the **manual GUI checklist (Appendix D)** in `npm run tauri dev` if at all
  possible — the B-tests cover structure, but exercise the real flows once.
- **Done when:** committed: `refactor(projects): split into Projects router + ProjectEditor`.

---

## Phase 2E — (optional) decompose ProjectEditor's tabs into panels

Only after 2D is green and merged. `ProjectEditor` is now self-contained, so its ~18
tabs can move into `projects/editor/panels/<Tab>Panel.tsx` with state shared via a
`useProjectEditor()` hook scoped INSIDE `ProjectEditor`. Extract easiest first
(`settings`, `groups`, `activity`, `recommendations`, docs tabs, `memory`), then medium
(`skills`, `mcp_servers`, `tools`, `summary`), then riskiest last (`rules`, `commands`,
`hooks`, `custom_agents`, `project_file`, `context`). One panel per commit; run
`npm test` + `tsc` each time. This is line-count cleanup, not an architectural necessity
— defer or skip if not worth it.

---

## Appendix A — `ProjectEditorProps` (exact)

```ts
interface ProjectEditorProps {
  selectedName: string | null;
  setSelectedName: (name: string | null) => void;
  isCreating: boolean;
  setIsCreating: (v: boolean) => void;
  reloadProjects: () => Promise<void>;
  setProjectDetailsMap: React.Dispatch<React.SetStateAction<Map<string, Project>>>;
  setDriftByProject: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  onBack: () => void;
  initialProjectTab?: string | null;
  onInitialProjectTabConsumed?: () => void;
  createFromTemplates?: ProjectTemplate[] | null;
  onCreateFromTemplatesConsumed?: () => void;
  onNavigateToSkill?: (skillName: string) => void;
  onNavigateToMcpServer?: (serverName: string) => void;
  onNavigateToSkillStore?: (skillId: string) => void;
  onNavigateToSkillStoreWithResult?: (result: { id: string; name: string; source: string; installs: number }) => void;
  onNavigateToDiscoverMcp?: (slug: string) => void;
  onNavigateToGroup?: (groupName: string) => void;
  onNavigateToCommand?: (commandId: string) => void;
}
```

## Appendix B — state/effect partition (router vs editor)

**Router keeps** (list + routing):
- State: `projects`, `projectsLoading`, `selectedName`, `isCreating`,
  `projectDetailsMap`, `driftByProject`, `syncAllStatus`, `createFromTemplates` (new).
- Functions: `applyStoredOrder`, `loadProjects`, `handleSyncAll`, `handleBackToOverview`.
- Effects: legacy-migration; background-drift-all (`[projects]`); `create-project`
  listener; `project-removed` listener; `resetKey`; `initialProject`;
  `initialCreateWithTemplate` resolution.
- Render: `<ProjectsOverview>`; shared modals if the list can open them.

**Editor gets everything else**, notably: `project`, `dirty`, all wizard state, all
`available*` resources, `pluginLocked*`, `syncStatus`, `driftReport`, `problemsReport`,
`rebuildPreview`, `unifiedSourcePicker`, project-templates UI state, `projectFiles`,
`projectVersion`, tab nav (`projectTab`/`projectGroup`/`toolTab`/`activeToolName`/
`returnView`, `PROJECT_GROUPS`, `PROJECT_CONTROLS`, `groupForTab`, `selectGroup`,
`selectTab`), memories, groups, recommendations, activity, context, docs, installed
editors. Handlers: `selectProject`, `reloadProject`, `updateField`, `handleSave`,
`handleSync`, `handleRebuild`, `startCreate`, `handleRemove`, rename, instruction-file,
template-apply, and all per-tab CRUD. Effects: per-project drift check
(`[selectedName, project?.directory, project?.agents.length, isCreating]`),
plugin-locked-resources, project-file load, activity load, rule-content warm, etc.

**Modals by screen** (render-site evidence in the pre-carve-out file):
- Shared (both branches): `DriftDiffModal`, `InstructionConflictModal`.
- Editor-only: `SwitchToUnifiedModal`, `RebuildConfirmationModal`,
  `ApplyProjectTemplateModal`.

## Appendix C — commands the tests will need to mock

`get_projects`, `read_project`, `save_project`, `delete_project`, `rename_project`,
`check_project_drift`, `check_project_problems`, `sync_project`, `list_agents`,
`get_skills`, `list_mcp_server_configs`, `get_templates`, `get_project_templates`,
`groups_for_project`, `list_groups`, `get_project_context`, `read_project_context_raw`,
`get_project_docs`, `get_project_activity*`, `list_tools_with_detection`,
`get_plugin_locked_resources`, `agent_features_enabled`, `read_settings`,
`check_installed_editors`. (Default the mock to return `[]`/`"{}"`/`undefined`; only
give meaningful data for the command a given test exercises.)

## Appendix D — manual GUI smoke checklist (run in `npm run tauri dev`)

1. List renders with project cards + drift badges.
2. Click a project → editor opens, tabs work, title correct.
3. Back → list; selection cleared.
4. New project (blank) → wizard steps 1→2→3 → save → lands in editor → appears in list.
5. New project from template → wizard seeded with template data → save → correct config.
6. Edit a project (add skill/agent), Save → "Saved & synced"; go back → overview drift
   badge updated (not stale).
7. Delete a project → confirm → removed from list.
8. Rename a project → editor + list reflect new name.
9. Trigger drift (edit a synced file on disk) → drift banner in editor; resolve via
   DriftDiffModal; resolve an instruction conflict via InstructionConflictModal.
10. Sync-all from the list works.

## Appendix E — risks to watch

- **Stale list after edit** — verify the prop setters (`setProjectDetailsMap`,
  `setDriftByProject`) and `reloadProjects` are wired at every editor mutation site.
- **Create flow** — the SEAM 2 mount effect must fire exactly once; double-check
  `initialCreateWithTemplate` seeding path.
- **State-lifetime change (intended):** editor state now resets on back (component
  unmounts) instead of persisting in the parent. This is the desired behavior; confirm
  nothing depended on persistence (e.g. returning to the same project mid-edit).
- Do NOT add `React.memo`/Context/`useCallback` "optimizations" — keep behavior identical.
