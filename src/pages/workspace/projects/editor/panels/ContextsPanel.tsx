import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, BookMarked, Check, Cloud, Layers, Plus, Search, X } from "lucide-react";
import type { Project } from "../../types";
import { providingGroup, summariseContext, type Context, type ContextTarget } from "../../../contexts/types";
import {
  ContextDialog,
  DialogError,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from "../../../contexts/ContextDialog";

interface ContextsPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  dirty: boolean;
  setDirty: (v: boolean) => void;
  /** In the create wizard the project is not on disk yet, so attach/detach only edit state. */
  isCreating: boolean;
  selectedName: string | null;
  reloadProject: (name: string) => Promise<void>;
  onNavigateToGroup?: (groupName: string) => void;
  /** Opens Library → Contexts, where contexts are created. */
  onNavigateToContexts?: () => void;
}

function ContextIcon({ context, size = 15 }: { context: Context | undefined; size?: number }) {
  return context?.location === "cloud" ? (
    <Cloud size={size} className="text-text-muted flex-shrink-0" />
  ) : (
    <BookMarked size={size} className="text-text-muted flex-shrink-0" />
  );
}

/**
 * The contexts this project's agents can read. Contexts a project group
 * provides show which group, and are removed from the group, not here.
 */
export function ContextsPanel({
  project, setProject, dirty, setDirty, isCreating, selectedName, reloadProject, onNavigateToGroup, onNavigateToContexts,
}: ContextsPanelProps) {
  const [library, setLibrary] = useState<Context[]>([]);
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadLibrary = async () => {
    try {
      const result: Context[] = await invoke("list_contexts");
      setLibrary([...result].sort((a, b) => (a.display_name || a.slug).localeCompare(b.display_name || b.slug)));
    } catch (err) {
      setError(`Couldn't load contexts: ${err}`);
    }
  };

  useEffect(() => {
    void loadLibrary();
    const handler = () => void loadLibrary();
    window.addEventListener("contexts-updated", handler);
    return () => window.removeEventListener("contexts-updated", handler);
  }, []);

  const attached = project.contexts ?? [];
  const contributions = project.group_context_contributions ?? {};
  const bySlug = new Map(library.map((c) => [c.slug, c]));

  // A clean, saved project goes through the backend so the change lands at
  // once. With unsaved edits (or in the wizard) only state changes, so a
  // reload cannot throw those edits away; the next save persists it.
  const direct = !isCreating && !!selectedName && !dirty;

  const change = async (slug: string, command: "attach_context" | "detach_context") => {
    setError(null);
    if (!direct) {
      const next = command === "attach_context" ? [...attached, slug] : attached.filter((c) => c !== slug);
      setProject({ ...project, contexts: next });
      setDirty(true);
      return;
    }
    setBusy(true);
    try {
      const target: ContextTarget = { type: "project", name: selectedName! };
      await invoke(command, { target, slug });
      await reloadProject(selectedName!);
    } finally {
      setBusy(false);
    }
  };

  const detach = async (slug: string) => {
    try {
      await change(slug, "detach_context");
    } catch (err) {
      setError(`Couldn't remove "${bySlug.get(slug)?.display_name || slug}": ${err}`);
    }
  };

  return (
    <section className="max-w-3xl space-y-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 className="text-[14px] font-medium text-text-base">Contexts</h3>
          <p className="text-[12px] text-text-muted mt-0.5">
            Reference material this project's agents can read when they need it. Nothing is copied into the project.
          </p>
        </div>
        <button onClick={() => setPicking(true)} disabled={busy} className={`${PRIMARY_BUTTON_CLASS} flex-shrink-0`}>
          <Plus size={12} /> Attach a context
        </button>
      </div>

      {error && (
        <div className="flex items-start gap-2 px-3 py-2 bg-danger/10 border border-danger/30 rounded-lg text-[12px] text-danger">
          <AlertTriangle size={13} className="flex-shrink-0 mt-0.5" />
          <span className="flex-1 break-words">{error}</span>
          <button onClick={() => setError(null)} className="text-danger/70 hover:text-danger" aria-label="Dismiss"><X size={12} /></button>
        </div>
      )}

      {attached.length === 0 ? (
        <div className="px-4 py-8 bg-bg-input border border-border-strong/40 rounded-lg text-center">
          <BookMarked size={20} className="mx-auto mb-2 text-text-muted" strokeWidth={1.5} />
          <p className="text-[13px] text-text-base mb-1">No contexts yet</p>
          <p className="text-[12px] text-text-muted mb-4">
            Attach coding standards, docs, or anything else agents should know about this project.
          </p>
          <div className="flex items-center justify-center gap-2">
            <button onClick={() => setPicking(true)} className={PRIMARY_BUTTON_CLASS}>
              <Plus size={12} /> Attach a context
            </button>
            {onNavigateToContexts && (
              <button onClick={onNavigateToContexts} className={SECONDARY_BUTTON_CLASS}>
                Create one in the Library
              </button>
            )}
          </div>
        </div>
      ) : (
        <ul className="border border-border-strong/40 rounded-lg bg-bg-input divide-y divide-border-strong/30">
          {attached.map((slug) => {
            const context = bySlug.get(slug);
            const group = providingGroup(contributions, slug);
            const missing = library.length > 0 && !context;
            return (
              <li key={slug} className="flex items-center gap-3 px-3 py-2.5">
                <ContextIcon context={context} />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] text-text-base truncate">{context?.display_name || slug}</div>
                  <div className="text-[11px] text-text-muted truncate">
                    {missing
                      ? "This context no longer exists in the Library."
                      : [context?.description, context ? summariseContext(context) : ""].filter(Boolean).join(" · ")}
                  </div>
                </div>
                {group ? (
                  <button
                    onClick={() => onNavigateToGroup?.(group)}
                    className="flex items-center gap-1 text-[11px] text-text-muted bg-bg-sidebar border border-border-strong/30 rounded px-1.5 py-0.5 hover:text-brand transition-colors flex-shrink-0"
                    title="This group provides it. Remove it from the group to take it off this project."
                  >
                    <Layers size={10} /> From group: {group}
                  </button>
                ) : (
                  <button
                    onClick={() => void detach(slug)}
                    disabled={busy}
                    className="p-1 text-text-muted hover:text-danger transition-colors flex-shrink-0 disabled:opacity-50"
                    aria-label={`Remove ${context?.display_name || slug}`}
                    title="Remove from this project"
                  >
                    <X size={13} />
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}

      {picking && (
        <AttachContextDialog
          library={library}
          attached={attached}
          onClose={() => setPicking(false)}
          onCreateNew={onNavigateToContexts}
          onAttach={async (slug) => {
            await change(slug, "attach_context");
            setPicking(false);
          }}
        />
      )}
    </section>
  );
}

function AttachContextDialog({
  library,
  attached,
  onClose,
  onCreateNew,
  onAttach,
}: {
  library: Context[];
  attached: string[];
  onClose: () => void;
  onCreateNew?: () => void;
  onAttach: (slug: string) => Promise<void>;
}) {
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const needle = filter.trim().toLowerCase();
  const visible = library.filter(
    (c) => !needle || c.slug.includes(needle) || c.display_name.toLowerCase().includes(needle) || c.description.toLowerCase().includes(needle),
  );

  const attach = async (slug: string | null = selected) => {
    if (!slug) return;
    setBusy(true);
    setError(null);
    try {
      await onAttach(slug);
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  return (
    <ContextDialog
      title="Attach a context"
      onClose={onClose}
      footer={
        <>
          {onCreateNew && (
            <button onClick={onCreateNew} className={`${SECONDARY_BUTTON_CLASS} mr-auto`}>
              <Plus size={12} /> New context
            </button>
          )}
          <button onClick={onClose} className={SECONDARY_BUTTON_CLASS}>Cancel</button>
          <button onClick={() => void attach()} disabled={!selected || busy} className={PRIMARY_BUTTON_CLASS}>Attach</button>
        </>
      }
    >
      {library.length === 0 ? (
        <p className="text-[13px] text-text-muted">
          There are no contexts in the Library yet. Create one first, then attach it here.
        </p>
      ) : (
        <>
          <div className="flex items-center gap-2 px-3 py-2 bg-bg-base border border-border-strong/40 rounded-md">
            <Search size={12} className="text-text-muted" />
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter contexts"
              aria-label="Filter contexts"
              autoFocus
              className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
            />
          </div>
          {visible.length === 0 ? (
            <p className="text-[12px] text-text-muted">Nothing matches.</p>
          ) : (
            <ul className="space-y-0.5">
              {visible.map((c) => {
                const isAttached = attached.includes(c.slug);
                const isSelected = selected === c.slug;
                return (
                  <li key={c.slug}>
                    <button
                      onClick={() => !isAttached && setSelected(c.slug)}
                      onDoubleClick={() => { if (!isAttached) { setSelected(c.slug); void attach(c.slug); } }}
                      disabled={isAttached}
                      className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md text-left transition-colors ${
                        isSelected ? "bg-brand/15 border border-brand/40" : "border border-transparent hover:bg-bg-sidebar"
                      } disabled:cursor-default disabled:hover:bg-transparent`}
                    >
                      <ContextIcon context={c} size={14} />
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] text-text-base truncate">{c.display_name || c.slug}</div>
                        <div className="text-[11px] text-text-muted truncate">{c.description || summariseContext(c)}</div>
                      </div>
                      {isAttached && (
                        <span className="flex items-center gap-1 text-[11px] text-text-muted"><Check size={11} /> Attached</span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </>
      )}
      <DialogError message={error} />
    </ContextDialog>
  );
}
