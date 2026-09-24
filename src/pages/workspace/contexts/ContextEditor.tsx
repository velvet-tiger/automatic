import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Cloud, Eye, MoreHorizontal, Trash2 } from "lucide-react";
import {
  ContextDialog,
  DialogError,
  FIELD_CLASS,
  HINT_CLASS,
  LABEL_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from "./ContextDialog";
import { LinkedMaterialSection } from "./LinkedMaterialSection";
import { PagesSection } from "./PagesSection";
import { SourcePreview } from "./SourcePreview";
import { UsedBySection } from "./UsedBySection";
import { displayName } from "./pageTree";
import { describeSaveStatus, useAutosave } from "./useAutosave";
import {
  PAGES_SOURCE_ID,
  isDocumentationSource,
  isValidSlug,
  uniqueSourceId,
  type Context,
  type ContextReferences,
  type ContextSource,
  type SourceSummary,
} from "./types";

interface ContextEditorProps {
  slug: string;
  /** Called after any saved change so the list can refresh. */
  onChanged: () => void;
  onRenamed: (newSlug: string) => void;
  onDeleted: () => void;
  onNavigateToProject?: (name: string) => void;
  onNavigateToGroup?: (name: string) => void;
}

/**
 * Edit one context. Every change saves on its own: text fields after a
 * short pause, everything else at once.
 */
export function ContextEditor({
  slug,
  onChanged,
  onRenamed,
  onDeleted,
  onNavigateToProject,
  onNavigateToGroup,
}: ContextEditorProps) {
  const [context, setContext] = useState<Context | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [changingId, setChangingId] = useState(false);
  // Saves always write the newest context, whichever edit scheduled them.
  const latest = useRef<Context | null>(null);

  const autosave = useAutosave<null>(async () => {
    if (!latest.current) return;
    await invoke("save_context", { context: latest.current });
    onChanged();
  }, 600);

  useEffect(() => {
    void (async () => {
      try {
        const loaded: Context = await invoke("read_context", { slug });
        latest.current = loaded;
        setContext(loaded);
      } catch (err) {
        setLoadError(String(err));
      }
    })();
  }, [slug]);

  if (loadError) {
    return <div className="p-6 text-[13px] text-danger">Couldn't open this context: {loadError}</div>;
  }
  if (!context) return <div className="p-6 text-[13px] text-text-muted">Loading…</div>;

  const editText = (patch: Partial<Pick<Context, "display_name" | "description">>) => {
    if (!latest.current) return;
    const next = { ...latest.current, ...patch } as Context;
    latest.current = next;
    setContext(next);
    autosave.schedule(null);
  };

  /** Replace the sources and save now. Restores the old list if the save fails. */
  const saveSources = async (sources: ContextSource[]) => {
    const previous = latest.current;
    if (!previous || previous.location !== "local") return;
    const next: Context = { ...previous, sources };
    latest.current = next;
    setContext(next);
    autosave.schedule(null, 0);
    try {
      await autosave.flushOrThrow();
    } catch (err) {
      latest.current = previous;
      setContext(previous);
      throw err;
    }
  };

  const ensurePagesSource = async (): Promise<string> => {
    const current = latest.current;
    if (!current || current.location !== "local") throw new Error("Cloud contexts have no local pages.");
    const existing = current.sources.find(isDocumentationSource);
    if (existing) return existing.id;
    const id = current.sources.some((s) => s.id === PAGES_SOURCE_ID)
      ? uniqueSourceId(PAGES_SOURCE_ID, current.sources)
      : PAGES_SOURCE_ID;
    await saveSources([...current.sources, { id, kind: "documentation", display_name: "", description: "" }]);
    return id;
  };

  const handleDelete = async () => {
    setMenuOpen(false);
    await autosave.flush();
    let refs: ContextReferences = { projects: [], groups: [] };
    try {
      refs = await invoke("get_context_references", { slug });
    } catch {
      // The confirmation still works without the count.
    }
    const uses = refs.projects.length + refs.groups.length;
    const usedNote = uses > 0 ? ` It will be detached from ${uses} project${uses === 1 ? "" : "s"} or group${uses === 1 ? "" : "s"}.` : "";
    const confirmed = await ask(
      `Delete "${context.display_name || context.slug}"?${usedNote} Its pages are deleted too. Linked folders, files and web pages aren't touched. This can't be undone.`,
      { title: "Delete context", kind: "warning" },
    );
    if (!confirmed) return;
    try {
      await invoke("delete_context", { slug });
      onDeleted();
    } catch (err) {
      setLoadError(`Couldn't delete: ${err}`);
    }
  };

  const docSources = context.location === "local" ? context.sources.filter(isDocumentationSource) : [];
  const saveText = describeSaveStatus(autosave.status);

  return (
    <div className="flex-1 flex flex-col h-full min-h-0">
      <div className="h-11 pl-6 pr-12 border-b border-border-strong/40 flex items-center justify-between gap-3">
        <span className="text-[12px] text-text-muted truncate">
          {context.location === "cloud" ? "Linked cloud context" : "Context"}
        </span>
        <div className="flex items-center gap-3">
          <span className={`text-[11px] ${autosave.status.kind === "error" ? "text-danger" : "text-text-muted"}`}>{saveText}</span>
          <div className="relative">
            <button onClick={() => setMenuOpen(!menuOpen)} className="p-1 text-text-muted hover:text-text-base rounded transition-colors" aria-label="More actions" aria-expanded={menuOpen}>
              <MoreHorizontal size={16} />
            </button>
            {menuOpen && (
              <div className="absolute right-0 top-full mt-1 w-48 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 py-1">
                <button
                  onClick={() => { setMenuOpen(false); setChangingId(true); }}
                  className="w-full text-left px-3 py-2 text-[12px] text-text-base hover:bg-bg-input transition-colors"
                >
                  Change ID…
                </button>
                <button onClick={() => void handleDelete()} className="w-full flex items-center gap-2 text-left px-3 py-2 text-[12px] text-danger hover:bg-bg-input transition-colors">
                  <Trash2 size={12} /> Delete context
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto custom-scrollbar p-6">
        <div className="max-w-3xl space-y-8">
          <div>
            <input
              value={context.display_name}
              onChange={(e) => editText({ display_name: e.target.value })}
              onBlur={() => void autosave.flush()}
              placeholder="Untitled context"
              aria-label="Name"
              className="w-full bg-transparent text-[20px] font-semibold text-text-base placeholder-text-muted/50 outline-none border-b border-transparent hover:border-border-strong/40 focus:border-brand/60 pb-1 transition-colors"
            />
            <textarea
              value={context.description}
              onChange={(e) => editText({ description: e.target.value })}
              onBlur={() => void autosave.flush()}
              placeholder="What is this context about? Agents read this first."
              aria-label="Description"
              rows={2}
              className="mt-2 w-full bg-transparent text-[13px] text-text-base placeholder-text-muted/50 outline-none resize-none border-b border-transparent hover:border-border-strong/40 focus:border-brand/60 transition-colors"
            />
          </div>

          {context.location === "local" ? (
            <>
              {docSources.length === 0 ? (
                <PagesSection contextSlug={context.slug} sourceId={null} heading="Pages" ensureSource={ensurePagesSource} />
              ) : (
                docSources.map((source) => (
                  <PagesSection
                    key={source.id}
                    contextSlug={context.slug}
                    sourceId={source.id}
                    heading={docSources.length === 1 ? "Pages" : source.display_name || displayName(source.id)}
                    ensureSource={async () => source.id}
                  />
                ))
              )}
              <LinkedMaterialSection contextSlug={context.slug} sources={context.sources} saveSources={saveSources} />
            </>
          ) : (
            <CloudSourcesSection contextSlug={context.slug} contextId={context.context_id} />
          )}

          <UsedBySection
            contextSlug={context.slug}
            onNavigateToProject={onNavigateToProject}
            onNavigateToGroup={onNavigateToGroup}
          />
        </div>
      </div>

      {changingId && (
        <ChangeIdDialog
          current={context.slug}
          onClose={() => setChangingId(false)}
          onChange={async (next) => {
            await autosave.flush();
            await invoke("rename_context", { oldSlug: context.slug, newSlug: next });
            setChangingId(false);
            onRenamed(next);
          }}
        />
      )}
    </div>
  );
}

function ChangeIdDialog({
  current,
  onClose,
  onChange,
}: {
  current: string;
  onClose: () => void;
  onChange: (next: string) => Promise<void>;
}) {
  const [value, setValue] = useState(current);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    const next = value.trim();
    if (next === current) return onClose();
    if (!isValidSlug(next)) {
      setError("Use lowercase letters, numbers, dots, dashes and underscores, starting with a letter or number.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onChange(next);
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  return (
    <ContextDialog
      title="Change context ID"
      onClose={onClose}
      footer={
        <>
          <button onClick={onClose} className={SECONDARY_BUTTON_CLASS}>Cancel</button>
          <button onClick={() => void submit()} disabled={busy} className={PRIMARY_BUTTON_CLASS}>Change ID</button>
        </>
      }
    >
      <div>
        <label className={LABEL_CLASS} htmlFor="context-id">ID</label>
        <input
          id="context-id"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") void submit(); }}
          autoFocus
          className={`${FIELD_CLASS} font-mono`}
        />
        <p className={HINT_CLASS}>
          Agents and the CLI refer to the context by this ID. Projects and groups that use it are updated for you.
        </p>
      </div>
      <DialogError message={error} />
    </ContextDialog>
  );
}

/** A linked cloud context: its web app id and the sources the web app reports. */
function CloudSourcesSection({ contextSlug, contextId }: { contextSlug: string; contextId: string }) {
  const [sources, setSources] = useState<SourceSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [previewId, setPreviewId] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const result: SourceSummary[] = await invoke("list_context_sources", { slug: contextSlug });
        setSources(result);
        setError(null);
      } catch (err) {
        setError(String(err));
      }
    })();
  }, [contextSlug]);

  return (
    <section>
      <h3 className="text-[14px] font-medium text-text-base">In the Automatic cloud</h3>
      <p className="text-[12px] text-text-muted mt-0.5 mb-2">
        This context lives in the web app ({contextId}). Agents read it as your signed-in account.
      </p>
      {error && (
        <div className="flex items-start gap-2 px-3 py-2 bg-danger/10 border border-danger/30 rounded-lg text-[12px] text-danger">
          <AlertTriangle size={13} className="flex-shrink-0 mt-0.5" />
          <span className="break-words">{error}</span>
        </div>
      )}
      {!error && sources === null && <p className="text-[12px] text-text-muted">Loading sources…</p>}
      {sources && sources.length === 0 && <p className="text-[12px] text-text-muted">No sources you can read.</p>}
      {sources && sources.length > 0 && (
        <ul className="border border-border-strong/40 rounded-lg bg-bg-input divide-y divide-border-strong/30">
          {sources.map((s) => (
            <li key={s.id}>
              <div className="flex items-center gap-3 px-3 py-2.5">
                <Cloud size={15} className="text-text-muted flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] text-text-base truncate">{s.display_name || s.id}</div>
                  <div className="text-[11px] text-text-muted truncate">{s.description || s.kind}</div>
                </div>
                <button
                  onClick={() => setPreviewId(previewId === s.id ? null : s.id)}
                  className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors"
                >
                  <Eye size={12} /> {previewId === s.id ? "Hide preview" : "Preview"}
                </button>
              </div>
              {previewId === s.id && (
                <div className="px-3 pb-3">
                  <SourcePreview contextSlug={contextSlug} sourceId={s.id} />
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
