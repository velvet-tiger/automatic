/**
 * Projects page — thin router between the project list (overview) and
 * the per-project editor.
 *
 * Phase 2D split (docs/projects-phase2-refactor.md): list + selection state
 * + cross-project effects live here; everything else lives in
 * `./projects/editor/ProjectEditor`. The editor is mounted on demand so its
 * ~7k lines of state are only realised while a project is selected or being
 * created.
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProjectsOverview } from "./projects/overview/ProjectsOverview";
import { ProjectEditor } from "./projects/editor/ProjectEditor";
import type { Project, ProjectTemplate, DriftReport } from "./projects/types";
import { trackProjectSynced } from "../../lib/analytics";

interface ProjectsProps {
  /** Increment to navigate back to the projects list (deselects any open project). */
  resetKey?: number;
  initialProject?: string | null;
  onInitialProjectConsumed?: () => void;
  /** When set, switch to this project tab immediately after selecting the project. */
  initialProjectTab?: string | null;
  onInitialProjectTabConsumed?: () => void;
  onNavigateToSkill?: (skillName: string) => void;
  onNavigateToMcpServer?: (serverName: string) => void;
  onNavigateToSkillStore?: (skillId: string) => void;
  onNavigateToSkillStoreWithResult?: (result: { id: string; name: string; source: string; installs: number }) => void;
  onNavigateToDiscoverMcp?: (slug: string) => void;
  onNavigateToGroup?: (groupName: string) => void;
  onNavigateToCommand?: (commandId: string) => void;
  /** When set, opens the new project wizard at step 3 with this template pre-selected. */
  initialCreateWithTemplate?: string | null;
  onInitialCreateWithTemplateConsumed?: () => void;
  /** When set, filters the overview to show only projects in this group. */
  filterGroup?: string | null;
}

const LAST_PROJECT_KEY = "automatic.projects.selected";
const PROJECT_ORDER_KEY = "automatic.projects.order";

export default function Projects({
  resetKey,
  initialProject = null,
  onInitialProjectConsumed,
  initialProjectTab = null,
  onInitialProjectTabConsumed,
  onNavigateToSkill,
  onNavigateToMcpServer,
  onNavigateToSkillStore,
  onNavigateToSkillStoreWithResult,
  onNavigateToDiscoverMcp,
  onNavigateToGroup,
  onNavigateToCommand,
  initialCreateWithTemplate = null,
  onInitialCreateWithTemplateConsumed,
  filterGroup = null,
}: ProjectsProps = {}) {
  // ── List state ────────────────────────────────────────────────────────────
  const [projects, setProjects] = useState<string[]>([]);
  const [projectsLoading, setProjectsLoading] = useState(true);
  const [projectDetailsMap, setProjectDetailsMap] = useState<Map<string, Project>>(new Map());
  const [driftByProject, setDriftByProject] = useState<Record<string, boolean>>({});
  const [syncAllStatus, setSyncAllStatus] = useState<"idle" | "syncing">("idle");

  // ── Selection / wizard state ─────────────────────────────────────────────
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  /** Resolved templates seeded into the wizard via initialCreateWithTemplate. */
  const [createFromTemplates, setCreateFromTemplates] = useState<ProjectTemplate[] | null>(null);

  // ── Legacy localStorage migration ────────────────────────────────────────
  useEffect(() => {
    const legacyKeys = [
      ["nexus.projects.selected", LAST_PROJECT_KEY],
      ["nexus.projects.order", PROJECT_ORDER_KEY],
    ];
    for (const [oldKey, newKey] of legacyKeys) {
      const val = localStorage.getItem(oldKey);
      if (val) {
        localStorage.setItem(newKey, val);
        localStorage.removeItem(oldKey);
      }
    }
  }, []);

  // ── Project list loader ──────────────────────────────────────────────────
  const applyStoredOrder = (names: string[]): string[] => {
    try {
      const stored = localStorage.getItem(PROJECT_ORDER_KEY);
      if (!stored) return names.sort();
      const order: string[] = JSON.parse(stored);
      const ordered: string[] = [];
      for (const n of order) {
        if (names.includes(n)) ordered.push(n);
      }
      const remaining = names.filter((n) => !ordered.includes(n)).sort();
      return [...ordered, ...remaining];
    } catch {
      return names.sort();
    }
  };

  const loadProjects = async (): Promise<void> => {
    try {
      const result: string[] = await invoke("get_projects");
      const ordered = applyStoredOrder(result);
      setProjects(ordered);
      const entries = await Promise.all(
        ordered.map(async (name) => {
          try {
            const raw: string = await invoke("read_project", { name });
            return [name, JSON.parse(raw) as Project] as const;
          } catch {
            return null;
          }
        }),
      );
      setProjectDetailsMap(new Map(entries.filter(Boolean) as [string, Project][]));
    } catch {
      // Failing the list load is surfaced via the empty grid; no global error UI here.
    } finally {
      setProjectsLoading(false);
    }
  };

  // Initial list load
  useEffect(() => {
    void loadProjects();
  }, []);

  // ── Background drift check for all projects (for sidebar/overview indicators) ─
  useEffect(() => {
    if (projects.length === 0) return;
    let cancelled = false;
    const checkAll = async () => {
      for (const name of projects) {
        if (cancelled) return;
        try {
          const raw: string = await invoke("check_project_drift", { name });
          const report = JSON.parse(raw) as DriftReport;
          if (!cancelled) {
            setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
          }
        } catch {
          // skip silently
        }
        await new Promise((res) => setTimeout(res, 200));
      }
    };
    void checkAll();
    const interval = setInterval(checkAll, 60_000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [projects]);

  // ── Window event listeners (cross-app navigation) ────────────────────────
  useEffect(() => {
    const handler = () => {
      setCreateFromTemplates(null);
      setSelectedName(null);
      setIsCreating(true);
    };
    window.addEventListener("create-project", handler);
    return () => window.removeEventListener("create-project", handler);
  }, []);

  useEffect(() => {
    const handler = (event: Event) => {
      const removedName = (event as CustomEvent<{ name?: string }>).detail?.name;
      if (!removedName) return;
      setSelectedName((current) => (current === removedName ? null : current));
      void loadProjects();
    };
    window.addEventListener("project-removed", handler);
    return () => window.removeEventListener("project-removed", handler);
  }, []);

  // resetKey: parent nav click while already on Projects → return to list
  useEffect(() => {
    if (resetKey === undefined || resetKey === 0) return;
    setSelectedName(null);
    setIsCreating(false);
    setCreateFromTemplates(null);
  }, [resetKey]);

  // initialProject: open this project's editor directly
  useEffect(() => {
    if (initialProject && projects.includes(initialProject)) {
      setSelectedName(initialProject);
      setIsCreating(false);
      onInitialProjectConsumed?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialProject, projects]);

  // initialCreateWithTemplate: resolve the template name, seed the wizard
  useEffect(() => {
    if (!initialCreateWithTemplate) return;
    let cancelled = false;
    (async () => {
      try {
        const names: string[] = await invoke("get_project_templates");
        if (cancelled || !names.includes(initialCreateWithTemplate)) {
          onInitialCreateWithTemplateConsumed?.();
          return;
        }
        const raw: string = await invoke("read_template", { name: initialCreateWithTemplate });
        const tmpl = JSON.parse(raw) as ProjectTemplate;
        if (cancelled) return;
        setCreateFromTemplates([tmpl]);
        setSelectedName(null);
        setIsCreating(true);
        onInitialCreateWithTemplateConsumed?.();
      } catch {
        onInitialCreateWithTemplateConsumed?.();
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialCreateWithTemplate]);

  // ── Handlers ─────────────────────────────────────────────────────────────
  const handleSyncAll = async () => {
    const driftedProjects = projects.filter((n) => driftByProject[n] === true);
    if (driftedProjects.length === 0) return;
    setSyncAllStatus("syncing");
    try {
      for (const name of driftedProjects) {
        try {
          const result: string = await invoke("sync_project", { name });
          const files: string[] = JSON.parse(result);
          trackProjectSynced(name);
          setDriftByProject((prev) => ({ ...prev, [name]: false }));
          void files;
        } catch {
          // continue
        }
      }
    } finally {
      setSyncAllStatus("idle");
    }
  };

  const handleBackToOverview = () => {
    setSelectedName(null);
    setIsCreating(false);
    setCreateFromTemplates(null);
    localStorage.removeItem(LAST_PROJECT_KEY);
  };

  // ── Render ───────────────────────────────────────────────────────────────
  if (!selectedName && !isCreating) {
    return (
      <div className="h-full w-full bg-bg-base overflow-hidden">
        <ProjectsOverview
          projects={projects}
          projectsLoading={projectsLoading}
          projectDetails={projectDetailsMap}
          driftByProject={driftByProject}
          onSelect={(name) => setSelectedName(name)}
          onCreate={() => {
            setCreateFromTemplates(null);
            setIsCreating(true);
          }}
          onSyncAll={handleSyncAll}
          syncAllStatus={syncAllStatus}
          filterGroup={filterGroup}
        />
      </div>
    );
  }

  return (
    <ProjectEditor
      selectedName={selectedName}
      setSelectedName={setSelectedName}
      isCreating={isCreating}
      setIsCreating={setIsCreating}
      reloadProjects={loadProjects}
      setProjectDetailsMap={setProjectDetailsMap}
      setDriftByProject={setDriftByProject}
      onBack={handleBackToOverview}
      initialProject={selectedName ? projectDetailsMap.get(selectedName) ?? null : null}
      initialProjectTab={initialProjectTab}
      onInitialProjectTabConsumed={onInitialProjectTabConsumed}
      createFromTemplates={createFromTemplates}
      onCreateFromTemplatesConsumed={() => setCreateFromTemplates(null)}
      onNavigateToSkill={onNavigateToSkill}
      onNavigateToMcpServer={onNavigateToMcpServer}
      onNavigateToSkillStore={onNavigateToSkillStore}
      onNavigateToSkillStoreWithResult={onNavigateToSkillStoreWithResult}
      onNavigateToDiscoverMcp={onNavigateToDiscoverMcp}
      onNavigateToGroup={onNavigateToGroup}
      onNavigateToCommand={onNavigateToCommand}
    />
  );
}
