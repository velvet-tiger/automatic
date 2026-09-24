import { useState } from "react";
import { open as openDialog, ask } from "@tauri-apps/plugin-dialog";
import { ChevronDown, Cloud, Eye, FilePlus, FolderOpen, FolderPlus, Globe, Link, Pencil, X } from "lucide-react";
import {
  ContextDialog,
  DialogError,
  FIELD_CLASS,
  HINT_CLASS,
  LABEL_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from "./ContextDialog";
import { SourcePreview } from "./SourcePreview";
import {
  DEFAULT_LOCAL_INCLUDE,
  DEFAULT_URL_TTL_SECS,
  sourceProblem,
  uniqueSourceId,
  type ContextSource,
  type LinkedSource,
} from "./types";

// How long a downloaded copy is reused before the next read downloads the
// page again. Nothing is fetched in the background.
const COPY_LIFETIME_OPTIONS: { value: number; label: string }[] = [
  { value: 0, label: "Don't keep a copy" },
  { value: 900, label: "15 minutes" },
  { value: 3600, label: "1 hour" },
  { value: 86400, label: "1 day" },
  { value: 604800, label: "1 week" },
];

function copyLifetimeLabel(ttl: number): string {
  if (ttl === 0) return "downloaded on every read";
  const match = COPY_LIFETIME_OPTIONS.find((o) => o.value === ttl);
  return `copy kept for ${match ? match.label : `${ttl} seconds`}`;
}

function baseName(path: string): string {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
}

function nameFromUrl(url: string): string {
  try {
    const parsed = new URL(url);
    const last = parsed.pathname.split("/").filter(Boolean).pop();
    return last ? `${parsed.hostname} ${decodeURIComponent(last)}` : parsed.hostname;
  } catch {
    return "";
  }
}

function describe(source: LinkedSource): string {
  switch (source.kind) {
    case "local":
      return source.config.path;
    case "url":
      return `${source.config.url} · ${copyLifetimeLabel(source.config.ttl_secs)}`;
    case "cloud":
      return `Automatic cloud · ${source.config.source_id}`;
  }
}

function KindIcon({ kind }: { kind: LinkedSource["kind"] }) {
  const cls = "text-text-muted flex-shrink-0";
  if (kind === "local") return <FolderOpen size={15} className={cls} />;
  if (kind === "url") return <Globe size={15} className={cls} />;
  return <Cloud size={15} className={cls} />;
}

type EditState = { source: LinkedSource; isNew: boolean } | null;

interface LinkedMaterialSectionProps {
  contextSlug: string;
  sources: ContextSource[];
  /** Replace the context's sources and save. Rejects with a readable message on failure. */
  saveSources: (next: ContextSource[]) => Promise<void>;
}

/**
 * Material that stays where it is: local folders and files, web pages, and
 * sources in the Automatic cloud. Agents read it on demand.
 */
export function LinkedMaterialSection({ contextSlug, sources, saveSources }: LinkedMaterialSectionProps) {
  const [editing, setEditing] = useState<EditState>(null);
  const [previewId, setPreviewId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const linked = sources.filter((s): s is LinkedSource => s.kind !== "documentation");

  const startLocal = async () => {
    const picked = await openDialog({ multiple: false, directory: true });
    if (typeof picked !== "string") return;
    const name = baseName(picked);
    setEditing({
      isNew: true,
      source: { id: uniqueSourceId(name, sources), kind: "local", display_name: name, description: "", config: { path: picked, include: [] } },
    });
  };

  const startNew = (kind: "url" | "cloud") => {
    const source: LinkedSource =
      kind === "url"
        ? { id: "", kind, display_name: "", description: "", config: { url: "", ttl_secs: DEFAULT_URL_TTL_SECS } }
        : { id: "", kind, display_name: "", description: "", config: { source_id: "" } };
    setEditing({ isNew: true, source });
  };

  const remove = async (source: LinkedSource) => {
    const confirmed = await ask(`Remove "${source.display_name || source.id}" from this context? The material itself isn't touched.`, {
      title: "Remove linked material",
      kind: "warning",
    });
    if (!confirmed) return;
    try {
      await saveSources(sources.filter((s) => s.id !== source.id));
      setError(null);
    } catch (err) {
      setError(`Couldn't remove it: ${err}`);
    }
  };

  const submit = async (source: LinkedSource, isNew: boolean) => {
    // Ids are internal: a new entry gets one from its name, an edit keeps its own.
    const withId: LinkedSource = isNew ? { ...source, id: uniqueSourceId(source.display_name || source.kind, sources) } : source;
    const next = isNew ? [...sources, withId] : sources.map((s) => (s.id === withId.id ? withId : s));
    await saveSources(next);
    setPreviewId(null);
  };

  return (
    <section>
      <h3 className="text-[14px] font-medium text-text-base">Linked material</h3>
      <p className="text-[12px] text-text-muted mt-0.5 mb-2">Agents read these where they are. Nothing is copied.</p>

      {error && <p className="mb-2 text-[12px] text-danger">{error}</p>}

      {linked.length > 0 && (
        <ul className="border border-border-strong/40 rounded-lg bg-bg-input divide-y divide-border-strong/30 mb-2">
          {linked.map((source) => (
            <li key={source.id}>
              <div className="group flex items-center gap-3 px-3 py-2.5">
                <KindIcon kind={source.kind} />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] text-text-base truncate">{source.display_name || source.id}</div>
                  <div className="text-[11px] text-text-muted font-mono truncate" title={describe(source)}>{describe(source)}</div>
                </div>
                <button
                  onClick={() => setPreviewId(previewId === source.id ? null : source.id)}
                  className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors"
                  aria-expanded={previewId === source.id}
                >
                  <Eye size={12} /> {previewId === source.id ? "Hide preview" : "Preview"}
                </button>
                <button onClick={() => setEditing({ source, isNew: false })} className="p-1 text-text-muted hover:text-text-base transition-colors" aria-label={`Edit ${source.display_name || source.id}`}>
                  <Pencil size={12} />
                </button>
                <button onClick={() => void remove(source)} className="p-1 text-text-muted hover:text-danger transition-colors" aria-label={`Remove ${source.display_name || source.id}`}>
                  <X size={13} />
                </button>
              </div>
              {previewId === source.id && (
                <div className="px-3 pb-3">
                  <SourcePreview contextSlug={contextSlug} sourceId={source.id} />
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      <div className="flex flex-wrap gap-2">
        <button onClick={() => void startLocal()} className={SECONDARY_BUTTON_CLASS}>
          <FolderPlus size={12} /> Add a folder or file
        </button>
        <button onClick={() => startNew("url")} className={SECONDARY_BUTTON_CLASS}>
          <Link size={12} /> Add a web page
        </button>
        <button onClick={() => startNew("cloud")} className={SECONDARY_BUTTON_CLASS}>
          <Cloud size={12} /> Add from Automatic cloud
        </button>
      </div>

      {editing && (
        <LinkedSourceDialog
          initial={editing.source}
          isNew={editing.isNew}
          onClose={() => setEditing(null)}
          onSubmit={async (source) => {
            await submit(source, editing.isNew);
            setEditing(null);
          }}
        />
      )}
    </section>
  );
}

function LinkedSourceDialog({
  initial,
  isNew,
  onClose,
  onSubmit,
}: {
  initial: LinkedSource;
  isNew: boolean;
  onClose: () => void;
  onSubmit: (source: LinkedSource) => Promise<void>;
}) {
  const [source, setSource] = useState<LinkedSource>(initial);
  const [customFiles, setCustomFiles] = useState(source.kind === "local" && (source.config.include?.length ?? 0) > 0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const titles: Record<LinkedSource["kind"], string> = {
    local: isNew ? "Add a folder or file" : "Edit folder or file",
    url: isNew ? "Add a web page" : "Edit web page",
    cloud: isNew ? "Add from Automatic cloud" : "Edit cloud source",
  };

  const submit = async () => {
    const named: LinkedSource = source.display_name.trim()
      ? { ...source, display_name: source.display_name.trim() }
      : { ...source, display_name: source.kind === "url" ? nameFromUrl(source.config.url) : source.display_name };
    if (!named.display_name) return setError("Give it a name.");
    // Ids are generated and never shown, so check only what the user entered.
    const problem = sourceProblem({ ...named, id: "probe" }, []);
    if (problem) return setError(problem);
    setBusy(true);
    setError(null);
    try {
      await onSubmit(named);
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  const pick = async (directory: boolean) => {
    if (source.kind !== "local") return;
    const picked = await openDialog({ multiple: false, directory });
    if (typeof picked !== "string") return;
    setSource({
      ...source,
      display_name: source.display_name || baseName(picked),
      config: { ...source.config, path: picked },
    });
  };

  return (
    <ContextDialog
      title={titles[source.kind]}
      onClose={onClose}
      footer={
        <>
          <button onClick={onClose} className={SECONDARY_BUTTON_CLASS}>Cancel</button>
          <button onClick={() => void submit()} disabled={busy} className={PRIMARY_BUTTON_CLASS}>{isNew ? "Add" : "Save"}</button>
        </>
      }
    >
      {source.kind === "local" && (
        <>
          <div>
            <label className={LABEL_CLASS}>Location</label>
            <div className="text-[12px] text-text-base font-mono break-all bg-bg-base border border-border-strong/40 rounded-md px-3 py-2">
              {source.config.path || "Nothing chosen yet"}
            </div>
            <div className="flex gap-2 mt-2">
              <button onClick={() => void pick(true)} className={SECONDARY_BUTTON_CLASS}><FolderOpen size={12} /> Choose folder</button>
              <button onClick={() => void pick(false)} className={SECONDARY_BUTTON_CLASS}><FilePlus size={12} /> Choose file</button>
            </div>
          </div>
          <div>
            <label className={LABEL_CLASS}>Which files</label>
            <div className="relative">
              <select
                value={customFiles ? "custom" : "default"}
                onChange={(e) => {
                  const custom = e.target.value === "custom";
                  setCustomFiles(custom);
                  if (!custom && source.kind === "local") setSource({ ...source, config: { ...source.config, include: [] } });
                }}
                aria-label="Which files"
                className="w-full appearance-none text-[13px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 pr-7 py-2 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
              >
                <option value="default">Markdown and text files</option>
                <option value="custom">Only files matching patterns…</option>
              </select>
              <ChevronDown size={12} className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted" />
            </div>
            {customFiles && (
              <>
                <textarea
                  value={(source.config.include ?? []).join("\n")}
                  onChange={(e) =>
                    source.kind === "local" &&
                    setSource({
                      ...source,
                      config: { ...source.config, include: e.target.value.split("\n").map((l) => l.trim()).filter(Boolean) },
                    })
                  }
                  placeholder={DEFAULT_LOCAL_INCLUDE.join("\n")}
                  rows={3}
                  aria-label="File patterns"
                  className={`${FIELD_CLASS} mt-2 font-mono text-[12px] resize-none`}
                />
                <p className={HINT_CLASS}>One pattern per line, such as **/*.md. Hidden files are never shared.</p>
              </>
            )}
          </div>
        </>
      )}

      {source.kind === "url" && (
        <>
          <div>
            <label className={LABEL_CLASS} htmlFor="linked-url">Web address</label>
            <input
              id="linked-url"
              type="url"
              value={source.config.url}
              onChange={(e) => source.kind === "url" && setSource({ ...source, config: { ...source.config, url: e.target.value.trim() } })}
              placeholder="https://example.com/docs/api.md"
              autoFocus
              className={FIELD_CLASS}
            />
          </div>
          <div>
            <label className={LABEL_CLASS}>Keep a downloaded copy for</label>
            <div className="relative">
              <select
                value={source.config.ttl_secs}
                onChange={(e) => source.kind === "url" && setSource({ ...source, config: { ...source.config, ttl_secs: Number(e.target.value) } })}
                aria-label="Keep a downloaded copy for"
                className="w-full appearance-none text-[13px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 pr-7 py-2 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
              >
                {COPY_LIFETIME_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
              </select>
              <ChevronDown size={12} className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted" />
            </div>
            <p className={HINT_CLASS}>
              The page is downloaded when an agent first reads it. Later reads use the copy until this time is up, then
              the next read downloads it again.
            </p>
          </div>
        </>
      )}

      {source.kind === "cloud" && (
        <div>
          <label className={LABEL_CLASS} htmlFor="linked-cloud">Cloud source id</label>
          <input
            id="linked-cloud"
            value={source.config.source_id}
            onChange={(e) => source.kind === "cloud" && setSource({ ...source, config: { source_id: e.target.value.trim() } })}
            placeholder="src_…"
            autoFocus
            className={`${FIELD_CLASS} font-mono`}
          />
          <p className={HINT_CLASS}>Shown on the source's page in the Automatic web app. You need to be signed in.</p>
        </div>
      )}

      <div>
        <label className={LABEL_CLASS} htmlFor="linked-name">Name</label>
        <input
          id="linked-name"
          value={source.display_name}
          onChange={(e) => setSource({ ...source, display_name: e.target.value })}
          placeholder={source.kind === "url" ? nameFromUrl(source.config.url) || "Public API reference" : "Architecture decisions"}
          className={FIELD_CLASS}
        />
      </div>
      <div>
        <label className={LABEL_CLASS} htmlFor="linked-note">Note for agents <span className="text-text-muted font-normal">(optional)</span></label>
        <input
          id="linked-note"
          value={source.description}
          onChange={(e) => setSource({ ...source, description: e.target.value })}
          placeholder="When should an agent look here?"
          className={FIELD_CLASS}
        />
      </div>
      <DialogError message={error} />
    </ContextDialog>
  );
}
