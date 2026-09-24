import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, BookMarked, Cloud, Plus, Search, X } from "lucide-react";
import { summariseContext, type Context, type ContextTarget } from "./types";

interface GroupContextsSectionProps {
  groupName: string;
  contexts: string[];
  /** Called after an attach or detach so the page can reload the group. */
  onChanged: () => void;
}

/**
 * Contexts attached to a project group. Every member project receives them;
 * the backend updates the members as part of each attach or detach.
 */
export function GroupContextsSection({ groupName, contexts, onChanged }: GroupContextsSectionProps) {
  const [library, setLibrary] = useState<Context[]>([]);
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadLibrary = async () => {
    try {
      const result: Context[] = await invoke("list_contexts");
      setLibrary([...result].sort((a, b) => a.slug.localeCompare(b.slug)));
    } catch (err) {
      setError(`Failed to load contexts: ${err}`);
    }
  };

  useEffect(() => {
    void loadLibrary();
    const handler = () => void loadLibrary();
    window.addEventListener("contexts-updated", handler);
    return () => window.removeEventListener("contexts-updated", handler);
  }, []);

  const bySlug = new Map(library.map((c) => [c.slug, c]));
  const unattached = library.filter((c) => !contexts.includes(c.slug));
  const needle = search.trim().toLowerCase();
  const filtered = needle
    ? unattached.filter((c) => c.slug.includes(needle) || c.display_name.toLowerCase().includes(needle))
    : unattached;

  const change = async (slug: string, command: "attach_context" | "detach_context") => {
    setAdding(false);
    setSearch("");
    setError(null);
    setBusy(true);
    try {
      const target: ContextTarget = { type: "group", name: groupName };
      await invoke(command, { target, slug });
      onChanged();
    } catch (err) {
      setError(`Failed to ${command === "attach_context" ? "attach" : "detach"} context "${slug}": ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span className="text-[12px] font-semibold text-text-muted uppercase tracking-wider">Contexts</span>
          <span className="text-[11px] text-text-muted bg-bg-sidebar px-1.5 rounded">{contexts.length}</span>
        </div>
        <div className="relative">
          <button
            onClick={() => setAdding(!adding)}
            disabled={busy}
            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium disabled:opacity-50"
          >
            <Plus size={12} /> Attach context
          </button>
          {adding && (
            <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
              <div className="p-2 border-b border-border-strong/40 flex items-center gap-2">
                <Search size={12} className="text-text-muted shrink-0" />
                <input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") { setAdding(false); setSearch(""); }
                    if (e.key === "Enter" && filtered.length === 1) void change(filtered[0]!.slug, "attach_context");
                  }}
                  placeholder="Search contexts..."
                  autoFocus
                  className="flex-1 bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                />
              </div>
              <div className="py-1">
                {filtered.length === 0 ? (
                  <div className="px-3 py-2 text-[12px] text-text-muted italic">
                    {library.length === 0
                      ? "No contexts in the library yet."
                      : unattached.length === 0
                        ? "All contexts already attached."
                        : "No contexts match."}
                  </div>
                ) : (
                  filtered.map((c) => (
                    <button
                      key={c.slug}
                      onClick={() => void change(c.slug, "attach_context")}
                      className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                    >
                      {c.location === "cloud" ? (
                        <Cloud size={14} className="text-text-muted flex-shrink-0" />
                      ) : (
                        <BookMarked size={14} className="text-text-muted flex-shrink-0" />
                      )}
                      <span className="text-[12px] font-medium text-text-base truncate">{c.display_name || c.slug}</span>
                    </button>
                  ))
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      {error && (
        <div className="mb-2 flex items-start gap-2 px-3 py-2 bg-danger/10 border border-danger/30 rounded-lg text-[12px] text-danger">
          <AlertTriangle size={13} className="flex-shrink-0 mt-0.5" />
          <span className="flex-1">{error}</span>
          <button onClick={() => setError(null)} className="text-danger/70 hover:text-danger"><X size={12} /></button>
        </div>
      )}

      {contexts.length === 0 ? (
        <p className="text-[13px] text-text-muted italic opacity-60">
          No contexts attached. Every project in the group receives the contexts you attach here.
        </p>
      ) : (
        <ul className="space-y-1.5">
          {contexts.map((slug) => {
            const context = bySlug.get(slug);
            return (
              <li
                key={slug}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-bg-input border border-border-strong/30"
              >
                {context?.location === "cloud" ? (
                  <Cloud size={13} className="text-text-muted shrink-0" />
                ) : (
                  <BookMarked size={13} className="text-text-muted shrink-0" />
                )}
                <span className="flex-1 min-w-0 text-[13px] text-text-base truncate">
                  {context?.display_name || slug}
                  <span className="ml-2 text-[11px] text-text-muted">
                    {context ? summariseContext(context) : library.length > 0 ? "Missing from library" : ""}
                  </span>
                </span>
                <button
                  onClick={() => void change(slug, "detach_context")}
                  disabled={busy}
                  className="flex items-center justify-center w-[20px] h-[20px] rounded text-text-muted hover:bg-red-500/10 hover:text-red-400 transition-colors shrink-0 disabled:opacity-50"
                  title={`Detach ${slug} from the group`}
                >
                  <X size={11} />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
