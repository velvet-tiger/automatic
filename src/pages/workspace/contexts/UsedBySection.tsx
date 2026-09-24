import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Folder, Layers, Plus, Search, X } from "lucide-react";
import { ContextDialog, DialogError, PRIMARY_BUTTON_CLASS, SECONDARY_BUTTON_CLASS } from "./ContextDialog";
import type { ContextReferences, ContextTarget } from "./types";

interface UsedBySectionProps {
  contextSlug: string;
  onNavigateToProject?: (name: string) => void;
  onNavigateToGroup?: (name: string) => void;
}

/** Where this context is attached, and a way to attach it somewhere else. */
export function UsedBySection({ contextSlug, onNavigateToProject, onNavigateToGroup }: UsedBySectionProps) {
  const [refs, setRefs] = useState<ContextReferences>({ projects: [], groups: [] });
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    try {
      const result: ContextReferences = await invoke("get_context_references", { slug: contextSlug });
      setRefs(result);
    } catch (err) {
      setError(`Couldn't load where this is used: ${err}`);
    }
  };

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
          {refs.projects.map((name) => (
            <Row
              key={`p:${name}`}
              icon={<Folder size={13} className="text-text-muted" />}
              label={name}
              note="Project"
              onOpen={() => onNavigateToProject?.(name)}
              onDetach={() => void detach({ type: "project", name })}
            />
          ))}
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
  label: string;
  note: string;
  onOpen: () => void;
  onDetach: () => void;
}) {
  return (
    <li className="group flex items-center gap-2.5 px-3 py-2">
      {icon}
      <button onClick={onOpen} className="flex-1 min-w-0 text-left text-[13px] text-text-base hover:text-brand truncate transition-colors">
        {label}
      </button>
      <span className="text-[11px] text-text-muted">{note}</span>
      <button onClick={onDetach} className="p-1 text-text-muted hover:text-danger transition-colors" aria-label={`Detach from ${label}`} title="Detach">
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
  const [projects, setProjects] = useState<string[]>([]);
  const [groups, setGroups] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<ContextTarget | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const [p, g] = await Promise.all([invoke<string[]>("get_projects"), invoke<string[]>("list_groups")]);
        setProjects([...p].sort((a, b) => a.localeCompare(b)));
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

  const option = (target: ContextTarget, isAttached: boolean) => {
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
          <span className="flex-1 text-[13px] text-text-base truncate">{target.name}</span>
          {isAttached && <span className="flex items-center gap-1 text-[11px] text-text-muted"><Check size={11} /> Attached</span>}
        </button>
      </li>
    );
  };

  const visibleGroups = groups.filter(match);
  const visibleProjects = projects.filter(match);

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
          <ul className="space-y-0.5">{visibleProjects.map((name) => option({ type: "project", name }, attached.projects.includes(name)))}</ul>
        </div>
      )}
      {visibleGroups.length === 0 && visibleProjects.length === 0 && (
        <p className="text-[12px] text-text-muted">{needle ? "Nothing matches." : "No projects or groups yet."}</p>
      )}
      <DialogError message={error} />
    </ContextDialog>
  );
}
