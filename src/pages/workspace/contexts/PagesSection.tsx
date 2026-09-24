import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderPlus,
  Maximize2,
  Minimize2,
  Move,
  Plus,
  Trash2,
} from "lucide-react";
import { LineNumberedTextarea } from "../../../components/LineNumberedTextarea";
import { MarkdownPreview } from "../../../components/MarkdownPreview";
import {
  ContextDialog,
  DialogError,
  FIELD_CLASS,
  HINT_CLASS,
  LABEL_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from "./ContextDialog";
import {
  buildPageTree,
  childFolderPath,
  displayName,
  folderPaths,
  isLastPageInFolder,
  joinFrontmatter,
  newPageContent,
  pageName,
  pagePath,
  parentFolder,
  splitFrontmatter,
  type PageFolder,
} from "./pageTree";
import { describeSaveStatus, useAutosave } from "./useAutosave";

interface PagesSectionProps {
  contextSlug: string;
  /** The documentation source holding the pages, or null until the first page is created. */
  sourceId: string | null;
  heading: string;
  /** Creates the pages source when there is none yet and returns its id. */
  ensureSource: () => Promise<string>;
}

interface OpenPage {
  path: string;
  frontmatter: string;
  body: string;
}

type EditorView = "write" | "preview";

type DialogState =
  | { type: "new-page"; folder: string }
  | { type: "new-folder"; parent: string }
  | { type: "move"; path: string }
  | null;

/**
 * Pages are notes stored by Automatic. Folders come from page paths, as in
 * the webapp, so a folder exists only while it holds a page. Edits save
 * automatically after a short pause.
 */
export function PagesSection({ contextSlug, sourceId, heading, ensureSource }: PagesSectionProps) {
  const [pages, setPages] = useState<string[]>([]);
  const [open, setOpen] = useState<OpenPage | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dialog, setDialog] = useState<DialogState>(null);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [view, setView] = useState<EditorView>("write");

  // While expanded, Esc closes the open dialog, or else collapses the editor.
  // It listens in the capture phase and stops the event, because the
  // surrounding AssetDrawer closes the whole context on any Esc.
  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      if (dialog !== null) setDialog(null);
      else setExpanded(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [expanded, dialog]);

  const writePage = async (value: { sourceId: string; path: string; content: string }) => {
    await invoke("write_context_documentation_page", {
      slug: contextSlug,
      sourceId: value.sourceId,
      path: value.path,
      content: value.content,
    });
  };
  const autosave = useAutosave(writePage, 800);

  const loadPages = async (id: string | null = sourceId): Promise<string[]> => {
    if (!id) {
      setPages([]);
      return [];
    }
    try {
      const result: string[] = await invoke("list_context_documentation_pages", { slug: contextSlug, sourceId: id });
      setPages(result);
      return result;
    } catch (err) {
      setError(`Couldn't load pages: ${err}`);
      return [];
    }
  };

  useEffect(() => {
    setOpen(null);
    setExpanded(false);
    void loadPages();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [contextSlug, sourceId]);

  // Deleting the last page removes the workspace, so leave the expanded view with it.
  useEffect(() => {
    if (pages.length === 0) setExpanded(false);
  }, [pages.length]);

  const tree = useMemo(() => buildPageTree(pages), [pages]);
  const folders = useMemo(() => folderPaths(pages), [pages]);

  const openPage = async (path: string, id: string | null = sourceId) => {
    if (!id) return;
    await autosave.flush();
    try {
      const content: string = await invoke("read_context_documentation_page", { slug: contextSlug, sourceId: id, path });
      setOpen({ path, ...splitFrontmatter(content) });
      setError(null);
    } catch (err) {
      setError(`Couldn't open page: ${err}`);
    }
  };

  const editBody = (body: string) => {
    if (!open || !sourceId) return;
    setOpen({ ...open, body });
    autosave.schedule({ sourceId, path: open.path, content: joinFrontmatter(open.frontmatter, body) });
  };

  /** Create a page at `path` and open it. */
  const createPage = async (path: string, title: string) => {
    await autosave.flush();
    const id = sourceId ?? (await ensureSource());
    await invoke("write_context_documentation_page", {
      slug: contextSlug,
      sourceId: id,
      path,
      content: newPageContent(title),
    });
    await loadPages(id);
    setCollapsed((prev) => {
      const next = new Set(prev);
      let folder = parentFolder(path);
      while (folder) {
        next.delete(folder);
        folder = parentFolder(folder);
      }
      return next;
    });
    await openPage(path, id);
  };

  const movePage = async (from: string, to: string) => {
    if (!sourceId || from === to) return;
    await autosave.flush();
    await invoke("move_context_documentation_page", { slug: contextSlug, sourceId, from, to });
    await loadPages();
    if (open?.path === from) setOpen({ ...open, path: to });
  };

  const deletePage = async (path: string) => {
    if (!sourceId) return;
    const folder = parentFolder(path);
    const folderNote = isLastPageInFolder(path, pages)
      ? ` It's the last page in "${displayName(folder.split("/").pop() ?? folder)}", so that folder goes too.`
      : "";
    const confirmed = await ask(`Delete "${pageName(path)}"?${folderNote} This can't be undone.`, {
      title: "Delete page",
      kind: "warning",
    });
    if (!confirmed) return;
    await autosave.flush();
    try {
      await invoke("delete_context_documentation_page", { slug: contextSlug, sourceId, path });
      if (open?.path === path) setOpen(null);
      await loadPages();
    } catch (err) {
      setError(`Couldn't delete page: ${err}`);
    }
  };

  const toggleFolder = (path: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const breadcrumb = open
    ? parentFolder(open.path)
        .split("/")
        .filter(Boolean)
        .map((segment) => displayName(segment))
    : [];

  const errorBanner = (
    <div className="mb-2 flex items-start gap-2 px-3 py-2 bg-danger/10 border border-danger/30 rounded-lg text-[12px] text-danger">
      <AlertTriangle size={13} className="flex-shrink-0 mt-0.5" />
      <span className="flex-1 break-words">{error}</span>
    </div>
  );

  const workspace = (
    <div className="h-full grid grid-cols-[minmax(0,15rem)_1fr] grid-rows-[minmax(0,1fr)] border border-border-strong/40 rounded-lg bg-bg-input overflow-hidden">
      <nav className="border-r border-border-strong/30 py-1.5 overflow-y-auto custom-scrollbar" aria-label="Pages">
        <TreeItems
          folder={tree}
          depth={0}
          openPath={open?.path ?? null}
          collapsed={collapsed}
          onOpen={(p) => void openPage(p)}
          onToggle={toggleFolder}
          onNewPageIn={(folder) => setDialog({ type: "new-page", folder })}
        />
      </nav>
      {open ? (
        <div className="flex flex-col min-w-0 min-h-0">
          <div className="flex items-center justify-between gap-2 px-3 py-2 border-b border-border-strong/30">
            <div className="text-[12px] text-text-muted truncate">
              {breadcrumb.map((part) => (
                <span key={part}>{part} / </span>
              ))}
              <span className="text-text-base">{pageName(open.path)}</span>
            </div>
            <div className="flex items-center gap-3 flex-shrink-0">
              <span className={`text-[11px] ${autosave.status.kind === "error" ? "text-danger" : "text-text-muted"}`}>
                {describeSaveStatus(autosave.status)}
              </span>
              <div className="flex items-center rounded border border-border-strong/40 overflow-hidden" role="group" aria-label="Editor view">
                {(["write", "preview"] as const).map((mode) => (
                  <button
                    key={mode}
                    onClick={() => setView(mode)}
                    aria-pressed={view === mode}
                    className={`px-2 py-0.5 text-[12px] transition-colors ${
                      view === mode ? "bg-brand/15 text-text-base" : "text-text-muted hover:text-text-base"
                    }`}
                  >
                    {mode === "write" ? "Write" : "Preview"}
                  </button>
                ))}
              </div>
              <button onClick={() => setDialog({ type: "move", path: open.path })} className="flex items-center gap-1 text-[12px] text-text-muted hover:text-text-base transition-colors">
                <Move size={12} /> Move
              </button>
              <button onClick={() => void deletePage(open.path)} className="flex items-center gap-1 text-[12px] text-text-muted hover:text-danger transition-colors">
                <Trash2 size={12} /> Delete
              </button>
              <button
                onClick={() => setExpanded((prev) => !prev)}
                className="flex items-center gap-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                title={expanded ? "Back to the context (Esc)" : "Edit in the full window"}
              >
                {expanded ? <><Minimize2 size={12} /> Collapse</> : <><Maximize2 size={12} /> Expand</>}
              </button>
            </div>
          </div>
          {view === "write" ? (
            <LineNumberedTextarea
              key={open.path}
              value={open.body}
              onChange={editBody}
              ariaLabel={`Content of ${pageName(open.path)}`}
              placeholder="Write in Markdown…"
              spellCheck
              className="flex-1 min-h-0"
            />
          ) : (
            <div className="flex-1 min-h-0 overflow-y-auto custom-scrollbar bg-bg-base">
              {open.body.trim() ? (
                <MarkdownPreview content={open.body} className="max-w-3xl px-6 py-5" />
              ) : (
                <p className="px-6 py-5 text-[12px] text-text-muted">Nothing to preview yet.</p>
              )}
            </div>
          )}
        </div>
      ) : (
        <div className="flex items-center justify-center p-6 text-[12px] text-text-muted">Select a page to read or edit it.</div>
      )}
    </div>
  );

  return (
    <section>
      <div className="flex items-center justify-between">
        <h3 className="text-[14px] font-medium text-text-base">{heading}</h3>
        <div className="flex items-center gap-2">
          <button onClick={() => setDialog({ type: "new-folder", parent: "" })} className={SECONDARY_BUTTON_CLASS}>
            <FolderPlus size={12} /> New folder
          </button>
          <button onClick={() => setDialog({ type: "new-page", folder: "" })} className={SECONDARY_BUTTON_CLASS}>
            <Plus size={12} /> New page
          </button>
        </div>
      </div>
      <p className="text-[12px] text-text-muted mt-0.5 mb-2">Notes you write here for agents to read.</p>

      {error && !expanded && errorBanner}

      {pages.length === 0 ? (
        <div className="px-4 py-6 bg-bg-input border border-border-strong/40 rounded-lg text-center">
          <FileText size={18} className="mx-auto mb-2 text-text-muted" strokeWidth={1.5} />
          <p className="text-[13px] text-text-base mb-1">Write your first page</p>
          <p className="text-[12px] text-text-muted mb-3">Conventions, decisions, or anything an agent should know.</p>
          <button onClick={() => setDialog({ type: "new-page", folder: "" })} className={`${PRIMARY_BUTTON_CLASS} mx-auto`}>
            <Plus size={12} /> New page
          </button>
        </div>
      ) : (
        !expanded && <div className="h-[70vh] min-h-[24rem]">{workspace}</div>
      )}

      {expanded && pages.length > 0 && (
        <div className="fixed inset-0 z-50 flex flex-col gap-3 bg-bg-base p-4" role="dialog" aria-modal="true" aria-label={`${heading}, full window`}>
          <div className="flex items-center justify-between">
            <h3 className="text-[14px] font-medium text-text-base">{heading}</h3>
            <div className="flex items-center gap-2">
              <button onClick={() => setDialog({ type: "new-folder", parent: "" })} className={SECONDARY_BUTTON_CLASS}>
                <FolderPlus size={12} /> New folder
              </button>
              <button onClick={() => setDialog({ type: "new-page", folder: "" })} className={SECONDARY_BUTTON_CLASS}>
                <Plus size={12} /> New page
              </button>
            </div>
          </div>
          {error && errorBanner}
          <div className="flex-1 min-h-0">{workspace}</div>
        </div>
      )}

      {dialog?.type === "new-page" && (
        <NewPageDialog
          folders={folders}
          initialFolder={dialog.folder}
          onClose={() => setDialog(null)}
          onCreate={async (folder, title) => {
            await createPage(pagePath(folder, title, pages), title);
            setDialog(null);
          }}
        />
      )}
      {dialog?.type === "new-folder" && (
        <NewFolderDialog
          folders={folders}
          initialParent={dialog.parent}
          onClose={() => setDialog(null)}
          onCreate={async (parent, name, title) => {
            await createPage(pagePath(childFolderPath(parent, name), title, pages), title);
            setDialog(null);
          }}
        />
      )}
      {dialog?.type === "move" && (
        <MovePageDialog
          path={dialog.path}
          folders={folders}
          pages={pages}
          onClose={() => setDialog(null)}
          onMove={async (to) => {
            await movePage(dialog.path, to);
            setDialog(null);
          }}
        />
      )}
    </section>
  );
}

function TreeItems({
  folder,
  depth,
  openPath,
  collapsed,
  onOpen,
  onToggle,
  onNewPageIn,
}: {
  folder: PageFolder;
  depth: number;
  openPath: string | null;
  collapsed: Set<string>;
  onOpen: (path: string) => void;
  onToggle: (path: string) => void;
  onNewPageIn: (folder: string) => void;
}) {
  const indent = { paddingLeft: `${10 + depth * 14}px` };
  return (
    <>
      {folder.folders.map((child) => {
        const isCollapsed = collapsed.has(child.path);
        return (
          <div key={child.path}>
            <div className="group flex items-center pr-2 hover:bg-bg-sidebar transition-colors" style={indent}>
              <button
                onClick={() => onToggle(child.path)}
                className="flex-1 min-w-0 flex items-center gap-1.5 py-1 text-left text-[12px] text-text-base"
                aria-expanded={!isCollapsed}
              >
                {isCollapsed ? <ChevronRight size={12} className="text-text-muted flex-shrink-0" /> : <ChevronDown size={12} className="text-text-muted flex-shrink-0" />}
                <Folder size={13} className="text-text-muted flex-shrink-0" />
                <span className="truncate">{child.name}</span>
              </button>
              <button
                onClick={() => onNewPageIn(child.path)}
                className="p-0.5 text-text-muted hover:text-brand opacity-0 group-hover:opacity-100 transition-opacity"
                aria-label={`New page in ${child.name}`}
                title={`New page in ${child.name}`}
              >
                <Plus size={12} />
              </button>
            </div>
            {!isCollapsed && (
              <TreeItems
                folder={child}
                depth={depth + 1}
                openPath={openPath}
                collapsed={collapsed}
                onOpen={onOpen}
                onToggle={onToggle}
                onNewPageIn={onNewPageIn}
              />
            )}
          </div>
        );
      })}
      {folder.pages.map((path) => (
        <button
          key={path}
          onClick={() => onOpen(path)}
          style={{ paddingLeft: `${10 + depth * 14 + (depth > 0 || folder.folders.length > 0 ? 18 : 0)}px` }}
          className={`w-full flex items-center gap-1.5 py-1 pr-2 text-left text-[12px] transition-colors ${
            openPath === path ? "bg-brand/15 text-text-base" : "text-text-base hover:bg-bg-sidebar"
          }`}
        >
          <FileText size={13} className="text-text-muted flex-shrink-0" />
          <span className="truncate">{pageName(path)}</span>
        </button>
      ))}
    </>
  );
}

/** "Guides / Troubleshooting" for a folder path. */
function folderLabel(path: string): string {
  return path ? path.split("/").map(displayName).join(" / ") : "Top level";
}

const NEW_FOLDER = "__new_folder__";

function FolderSelect({
  value,
  folders,
  onChange,
  allowNew,
  label,
}: {
  value: string;
  folders: string[];
  onChange: (value: string) => void;
  allowNew: boolean;
  label: string;
}) {
  return (
    <div className="relative">
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-label={label}
        className="w-full appearance-none text-[13px] text-text-base bg-bg-input border border-border-strong/50 rounded-md px-2.5 pr-7 py-2 focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 transition-colors"
      >
        <option value="">Top level</option>
        {folders.map((f) => (
          <option key={f} value={f}>{folderLabel(f)}</option>
        ))}
        {allowNew && <option value={NEW_FOLDER}>New folder…</option>}
      </select>
      <ChevronDown size={12} className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-muted" />
    </div>
  );
}

/** Run a dialog action, turning a thrown error into an inline message. */
function useDialogAction() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, setError, run };
}

function NewPageDialog({
  folders,
  initialFolder,
  onClose,
  onCreate,
}: {
  folders: string[];
  initialFolder: string;
  onClose: () => void;
  onCreate: (folder: string, title: string) => Promise<void>;
}) {
  const [title, setTitle] = useState("");
  const [folder, setFolder] = useState(initialFolder);
  const [newFolderName, setNewFolderName] = useState("");
  const action = useDialogAction();

  const submit = () => {
    if (!title.trim()) return action.setError("Give the page a title.");
    if (folder === NEW_FOLDER && !newFolderName.trim()) return action.setError("Name the new folder.");
    const target = folder === NEW_FOLDER ? childFolderPath("", newFolderName) : folder;
    void action.run(() => onCreate(target, title.trim()));
  };

  return (
    <ContextDialog
      title="New page"
      onClose={onClose}
      footer={
        <>
          <button onClick={onClose} className={SECONDARY_BUTTON_CLASS}>Cancel</button>
          <button onClick={submit} disabled={action.busy} className={PRIMARY_BUTTON_CLASS}>Create page</button>
        </>
      }
    >
      <div>
        <label className={LABEL_CLASS} htmlFor="new-page-title">Title</label>
        <input
          id="new-page-title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") submit(); }}
          placeholder="Setup guide"
          autoFocus
          className={FIELD_CLASS}
        />
      </div>
      <div>
        <label className={LABEL_CLASS}>Folder</label>
        <FolderSelect value={folder} folders={folders} onChange={setFolder} allowNew label="Folder" />
        {folder === NEW_FOLDER && (
          <input
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            placeholder="Guides"
            aria-label="New folder name"
            className={`${FIELD_CLASS} mt-2`}
          />
        )}
      </div>
      <DialogError message={action.error} />
    </ContextDialog>
  );
}

function NewFolderDialog({
  folders,
  initialParent,
  onClose,
  onCreate,
}: {
  folders: string[];
  initialParent: string;
  onClose: () => void;
  onCreate: (parent: string, name: string, firstPageTitle: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [parent, setParent] = useState(initialParent);
  const [title, setTitle] = useState("");
  const action = useDialogAction();

  const submit = () => {
    if (!name.trim()) return action.setError("Name the folder.");
    if (!title.trim()) return action.setError("Give the first page a title.");
    void action.run(() => onCreate(parent, name.trim(), title.trim()));
  };

  return (
    <ContextDialog
      title="New folder"
      onClose={onClose}
      footer={
        <>
          <button onClick={onClose} className={SECONDARY_BUTTON_CLASS}>Cancel</button>
          <button onClick={submit} disabled={action.busy} className={PRIMARY_BUTTON_CLASS}>Create folder</button>
        </>
      }
    >
      <div>
        <label className={LABEL_CLASS} htmlFor="new-folder-name">Folder name</label>
        <input id="new-folder-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Guides" autoFocus className={FIELD_CLASS} />
      </div>
      <div>
        <label className={LABEL_CLASS}>Inside</label>
        <FolderSelect value={parent} folders={folders} onChange={setParent} allowNew={false} label="Parent folder" />
      </div>
      <div>
        <label className={LABEL_CLASS} htmlFor="new-folder-page">First page</label>
        <input
          id="new-folder-page"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") submit(); }}
          placeholder="Overview"
          className={FIELD_CLASS}
        />
        <p className={HINT_CLASS}>A folder always holds at least one page, here and in the Automatic cloud.</p>
      </div>
      <DialogError message={action.error} />
    </ContextDialog>
  );
}

function MovePageDialog({
  path,
  folders,
  pages,
  onClose,
  onMove,
}: {
  path: string;
  folders: string[];
  pages: string[];
  onClose: () => void;
  onMove: (to: string) => Promise<void>;
}) {
  const [name, setName] = useState(pageName(path));
  const [folder, setFolder] = useState(parentFolder(path));
  const [newFolderName, setNewFolderName] = useState("");
  const action = useDialogAction();

  const submit = () => {
    if (!name.trim()) return action.setError("Give the page a name.");
    if (folder === NEW_FOLDER && !newFolderName.trim()) return action.setError("Name the new folder.");
    const target = folder === NEW_FOLDER ? childFolderPath("", newFolderName) : folder;
    const unchanged = target === parentFolder(path) && name.trim() === pageName(path);
    if (unchanged) return onClose();
    const to = pagePath(target, name.trim(), pages.filter((p) => p !== path));
    void action.run(() => onMove(to));
  };

  return (
    <ContextDialog
      title="Move or rename page"
      onClose={onClose}
      footer={
        <>
          <button onClick={onClose} className={SECONDARY_BUTTON_CLASS}>Cancel</button>
          <button onClick={submit} disabled={action.busy} className={PRIMARY_BUTTON_CLASS}>Move page</button>
        </>
      }
    >
      <div>
        <label className={LABEL_CLASS} htmlFor="move-page-name">Name</label>
        <input id="move-page-name" value={name} onChange={(e) => setName(e.target.value)} autoFocus className={FIELD_CLASS} />
      </div>
      <div>
        <label className={LABEL_CLASS}>Folder</label>
        <FolderSelect value={folder} folders={folders} onChange={setFolder} allowNew label="Destination folder" />
        {folder === NEW_FOLDER && (
          <input
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            placeholder="Guides"
            aria-label="New folder name"
            className={`${FIELD_CLASS} mt-2`}
          />
        )}
      </div>
      <DialogError message={action.error} />
    </ContextDialog>
  );
}
