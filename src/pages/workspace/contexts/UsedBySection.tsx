import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Folder, Layers, Plus, Search, X } from "lucide-react";
import { ContextDialog, DialogError, PRIMARY_BUTTON_CLASS, SECONDARY_BUTTON_CLASS } from "./ContextDialog";
import type { ContextProjectRef, ContextReferences, ContextTarget } from "./types";
import type { ProjectSummary } from "../projects/types";
import {
  loadProjectSummaries,
  projectKeyOf,
  projectLabelFor,
  projectLabels,
  projectLabelText,
  type ProjectLabel,
} from "../../../lib/projectIdentity";
import { ProjectNameLabel } from "../../../components/ProjectNameLabel";

/** The referenced projects, each with the key that addresses it: its local_key, else its name. */
function referencedProjects(refs: ContextReferences): { key: string; ref: ContextProjectRef }[] {
  const list: ContextProjectRef[] = refs.project_refs ?? refs.projects.map((name) => ({ name, directory: "" }));
  return list.map((ref) => ({ key: ref.local_key || ref.name, ref }));
}

interface UsedBySectionProps {
  contextSlug: string;
  /** Opens a project by local_key (or by name for an orphan reference). */
  onNavigateToProject?: (projectKey: string) => void;
  onNavigateToGroup?: (name: string) => void;
}

/** Where this context is attached, and a way to attach it somewhere else. */
export function UsedBySection({ contextSlug, onNavigateToProject, onNavigateToGroup }: UsedBySectionProps) {
  const [refs, setRefs] = useState<ContextReferences>({ projects: [], groups: [] });
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Every registered project, only to label duplicated names with their folder.
  const [summaries, setSummaries] = useState<ProjectSummary[]>([]);
  const labels = useMemo(() => projectLabels(summaries), [summaries]);

  const load = async () => {
    try {
      const result: ContextReferences = await invoke("get_context_references", { slug: contextSlug });
      setRefs(result);
    } catch (err) {
      setError(`Couldn't load where this is used: ${err}`);
    }
  };

  useEffect(() => {
    loadProjectSummaries()
      .then(setSummaries)
      .catch((err: unknown) => {
        // Non-fatal: rows fall back to plain project names.
        console.error("Failed to load project summaries:", err);
      });
  }, []);

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [contextSlug]);

  const detach = async (target: ContextTarget) => {
    try {
      await invoke("detach_context", { target, slug: contextSlug });
      setError(null);
      await load();
      window.dispatchEvent(new CustomEvent("groups-updated"));
    } catch (err) {
      setError(String(err));
    }
  };

  const empty = refs.projects.length === 0 && refs.groups.length === 0;

  return (
    <section>
      <div className="flex items-center justify-between">
        <h3 className="text-[14px] font-medium text-text-base">Used by</h3>
        <button onClick={() => setPicking(true)} className={SECONDARY_BUTTON_CLASS}>
          <Plus size={12} /> Attach to a project or group
        </button>
      </div>
      {error && <p className="mt-2 text-[12px] text-danger">{error}</p>}
      {empty ? (
        <p className="text-[12px] text-text-muted mt-1">Not attached yet. Agents in attached projects can read this context.</p>
      ) : (
        <ul className="mt-2 border border-border-strong/40 rounded-lg bg-bg-input divide-y divide-border-strong/30">
          {refs.groups.map((name) => (
            <Row
              key={`g:${name}`}
              icon={<Layers size={13} className="text-text-muted" />}
              label={name}
              note="Group · every project in it"
              onOpen={() => onNavigateToGroup?.(name)}
              onDetach={() => void detach({ type: "group", name })}
            />
          ))}
          {referencedProjects(refs).map(({ key, ref }) => {
            return (
              <Row
                key={`p:${key}`}
                icon={<Folder size={13} className="text-text-muted" />}
                label={projectLabelFor(labels, key, ref.name)}
                note="Project"
                onOpen={() => onNavigateToProject?.(key)}
                onDetach={() => void detach({ type: "project", name: key })}
              />
            );
          })}
        </ul>
      )}
      {picking && (
        <AttachDialog
          contextSlug={contextSlug}
          attached={refs}
          onClose={() => setPicking(false)}
          onAttached={async () => {
            await load();
            window.dispatchEvent(new CustomEvent("groups-updated"));
          }}
        />
      )}
    </section>
  );
}

function Row({
  icon,
  label,
  note,
  onOpen,
  onDetach,
}: {
  icon: ReactNode;
  label: string | ProjectLabel;
  note: string;
  onOpen: () => void;
  onDetach: () => void;
}) {
  const text = typeof label === "string" ? label : projectLabelText(label);
  return (
    <li className="group flex items-center gap-2.5 px-3 py-2">
      {icon}
      <button onClick={onOpen} className="flex-1 min-w-0 text-left text-[13px] text-text-base hover:text-brand truncate transition-colors">
        {typeof label === "string" ? label : <ProjectNameLabel label={label} />}
      </button>
      <span className="text-[11px] text-text-muted">{note}</span>
      <button onClick={onDetach} className="p-1 text-text-muted hover:text-danger transition-colors" aria-label={`Detach from ${text}`} title="Detach">
        <X size={12} />
      </button>
    </li>
  );
}

function AttachDialog({
  contextSlug,
  attached,
  onClose,
  onAttached,
}: {
  contextSlug: string;
  attached: ContextReferences;
  onClose: () => void;
  onAttached: () => Promise<void>;
}) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [groups, setGroups] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<ContextTarget | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const labels = useMemo(() => projectLabels(projects), [projects]);

  useEffect(() => {
    void (async () => {
      try {
        const [p, g] = await Promise.all([loadProjectSummaries(), invoke<string[]>("list_groups")]);
        setProjects([...p].sort((a, b) => a.name.localeCompare(b.name)));
        setGroups([...g].sort((a, b) => a.localeCompare(b)));
      } catch (err) {
        setError(`Couldn't load projects and groups: ${err}`);
      }
    })();
  }, []);

  const needle = filter.trim().toLowerCase();
  const match = (name: string) => !needle || name.toLowerCase().includes(needle);

  const attach = async (target: ContextTarget | null = selected) => {
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("attach_context", { target, slug: contextSlug });
      await onAttached();
      onClose();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  /** `label` is shown; a project target's `name` holds its local_key. */
  const option = (target: ContextTarget, isAttached: boolean, label: string | ProjectLabel = target.name) => {
    const isSelected = selected?.type === target.type && selected.name === target.name;
    return (
      <li key={`${target.type}:${target.name}`}>
        <button
          onClick={() => !isAttached && setSelected(target)}
          onDoubleClick={() => { if (!isAttached) { setSelected(target); void attach(target); } }}
          disabled={isAttached}
          className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md text-left transition-colors ${
            isSelected ? "bg-brand/15 border border-brand/40" : "border border-transparent hover:bg-bg-sidebar"
          } disabled:cursor-default disabled:hover:bg-transparent`}
        >
          {target.type === "group" ? <Layers size={13} className="text-text-muted" /> : <Folder size={13} className="text-text-muted" />}
          {typeof label === "string" ? (
            <span className="flex-1 text-[13px] text-text-base truncate">{label}</span>
          ) : (
            <ProjectNameLabel label={label} className="flex-1 text-[13px] text-text-base truncate" />
          )}
          {isAttached && <span className="flex items-center gap-1 text-[11px] text-text-muted"><Check size={11} /> Attached</span>}
        </button>
      </li>
    );
  };

  const visibleGroups = groups.filter(match);
  const visibleProjects = projects.filter((p) => match(projectLabelText(projectLabelFor(labels, projectKeyOf(p), p.name))));
  const attachedProjectKeys = new Set(referencedProjects(attached).map(({ key }) => key));

  return (
    <ContextDialog
      title="Attach context"
      onClose={onClose}
      footer={
        <>
          <button onClick={onClose} className={SECONDARY_BUTTON_CLASS}>Cancel</button>
          <button onClick={() => void attach()} disabled={!selected || busy} className={PRIMARY_BUTTON_CLASS}>Attach</button>
        </>
      }
    >
      <p className="text-[12px] text-text-muted">
        Attach to a group to reach every project in it, or to a single project.
      </p>
      <div className="flex items-center gap-2 px-3 py-2 bg-bg-base border border-border-strong/40 rounded-md">
        <Search size={12} className="text-text-muted" />
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter"
          aria-label="Filter projects and groups"
          autoFocus
          className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
        />
      </div>
      {visibleGroups.length > 0 && (
        <div>
          <p className="text-[11px] font-medium text-text-muted mb-1">Groups</p>
          <ul className="space-y-0.5">{visibleGroups.map((name) => option({ type: "group", name }, attached.groups.includes(name)))}</ul>
        </div>
      )}
      {visibleProjects.length > 0 && (
        <div>
          <p className="text-[11px] font-medium text-text-muted mb-1">Projects</p>
          <ul className="space-y-0.5">{visibleProjects.map((p) => {
            const key = projectKeyOf(p);
            return option({ type: "project", name: key }, attachedProjectKeys.has(key), projectLabelFor(labels, key, p.name));
          })}</ul>
        </div>
      )}
      {visibleGroups.length === 0 && visibleProjects.length === 0 && (
        <p className="text-[12px] text-text-muted">{needle ? "Nothing matches." : "No projects or groups yet."}</p>
      )}
      <DialogError message={error} />
    </ContextDialog>
  );
}
