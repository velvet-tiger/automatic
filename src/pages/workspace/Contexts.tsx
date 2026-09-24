import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BookMarked, Cloud, Link, Plus, Search, X } from "lucide-react";
import { AssetTable } from "../../components/AssetTable";
import { AssetDrawer } from "../../components/AssetDrawer";
import { ContextEditor } from "./contexts/ContextEditor";
import { NewContextDialog } from "./contexts/NewContextDialog";
import { summariseContext, type Context } from "./contexts/types";

// ── Contexts ──────────────────────────────────────────────────────────────────
//
// A context is reference material agents read on demand through MCP: pages
// written here, linked folders, files and web pages, or a context from the
// Automatic cloud. The editor saves every change on its own. Other views
// listen for `contexts-updated` to refresh.

function notifyContextsUpdated() {
  window.dispatchEvent(new CustomEvent("contexts-updated"));
}

export default function Contexts({
  onNavigateToProject,
  onNavigateToGroup,
}: {
  onNavigateToProject?: (projectName: string) => void;
  onNavigateToGroup?: (groupName: string) => void;
}) {
  const [contexts, setContexts] = useState<Context[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  const [creating, setCreating] = useState<"local" | "cloud" | null>(null);

  const loadContexts = async () => {
    try {
      const result: Context[] = await invoke("list_contexts");
      setContexts([...result].sort((a, b) => (a.display_name || a.slug).localeCompare(b.display_name || b.slug)));
      setError(null);
    } catch (err) {
      setError(`Couldn't load contexts: ${err}`);
    } finally {
      setLoaded(true);
    }
  };

  useEffect(() => {
    void loadContexts();
  }, []);

  const changed = () => {
    void loadContexts();
    notifyContextsUpdated();
  };

  const needle = search.trim().toLowerCase();
  const filtered = useMemo(
    () =>
      contexts.filter(
        (c) =>
          !needle ||
          c.slug.includes(needle) ||
          c.display_name.toLowerCase().includes(needle) ||
          c.description.toLowerCase().includes(needle),
      ),
    [contexts, needle],
  );

  const renderRow = (ctx: Context) => (
    <tr
      key={ctx.slug}
      onClick={() => setSelectedSlug(ctx.slug)}
      className={`cursor-pointer border-b border-border-strong/20 last:border-b-0 transition-colors ${
        selectedSlug === ctx.slug ? "bg-bg-sidebar/60" : "hover:bg-bg-input/70"
      }`}
    >
      <td className="px-3 py-2 w-11">
        <div className="w-8 h-8 rounded-md bg-brand/15 flex items-center justify-center">
          {ctx.location === "cloud" ? <Cloud size={15} className="text-brand" /> : <BookMarked size={15} className="text-brand" />}
        </div>
      </td>
      {/* max-w-0 stops the long description from sizing the column, so truncate cuts it at the window edge. */}
      <td className="px-3 py-2 w-full max-w-0">
        <div className="text-[13px] font-medium text-text-base truncate">{ctx.display_name || ctx.slug}</div>
        {ctx.description && (
          <div className="text-[11px] text-text-muted truncate" title={ctx.description}>
            {ctx.description}
          </div>
        )}
      </td>
      <td className="px-3 py-2 text-[11px] text-text-muted whitespace-nowrap">{summariseContext(ctx)}</td>
    </tr>
  );

  return (
    <div className="flex h-full w-full flex-col bg-bg-base">
      <div className="shrink-0 border-b border-border-strong/40 bg-bg-input/40">
        <div className="flex items-center justify-between px-4 pt-3 pb-2 gap-3">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Contexts</span>
          <div className="flex items-center gap-2 shrink-0">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              <input
                type="text"
                placeholder="Search contexts…"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="w-56 h-7 pl-7 pr-7 rounded-md bg-bg-input border border-border-strong/50 hover:border-border-strong focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 text-[12px] text-text-base placeholder-text-muted/60 transition-colors"
              />
              {search && (
                <button onClick={() => setSearch("")} aria-label="Clear search" className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors">
                  <X size={11} />
                </button>
              )}
            </div>
            <button
              onClick={() => setCreating("cloud")}
              className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-bg-input border border-border-strong/50 hover:border-border-strong text-text-base text-[12px] transition-colors"
            >
              <Link size={12} /> Link cloud context
            </button>
            <button
              onClick={() => setCreating("local")}
              className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-brand hover:bg-brand-hover text-white text-[12px] font-medium transition-colors"
            >
              <Plus size={12} /> New context
            </button>
          </div>
        </div>
      </div>

      {error && (
        <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between shrink-0">
          <span className="break-all">{error}</span>
          <button onClick={() => setError(null)} aria-label="Dismiss"><X size={14} /></button>
        </div>
      )}

      <AssetTable
        items={filtered}
        getId={(c) => c.slug}
        isEmpty={loaded && contexts.length === 0}
        emptyState={
          <>
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-brand/12 border border-brand/20 flex items-center justify-center">
              <BookMarked size={22} className="text-brand" strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">Give your agents some background</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              A context holds pages you write, plus folders, files and web pages agents should read. Attach it to projects
              and agents read it when they need it.
            </p>
            <button
              onClick={() => setCreating("local")}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
            >
              <Plus size={14} /> New context
            </button>
          </>
        }
        noMatchState={<p className="text-[13px] text-text-muted">No contexts match "{search}".</p>}
        columns={[
          { key: "icon", header: "", className: "w-11" },
          { key: "name", header: "Name" },
          { key: "summary", header: "" },
        ]}
        renderRow={renderRow}
      />

      <AssetDrawer open={!!selectedSlug} onClose={() => setSelectedSlug(null)}>
        {selectedSlug && (
          <ContextEditor
            key={selectedSlug}
            slug={selectedSlug}
            onChanged={changed}
            onRenamed={(next) => {
              setSelectedSlug(next);
              changed();
            }}
            onDeleted={() => {
              setSelectedSlug(null);
              changed();
            }}
            onNavigateToProject={onNavigateToProject}
            onNavigateToGroup={onNavigateToGroup}
          />
        )}
      </AssetDrawer>

      {creating && (
        <NewContextDialog
          mode={creating}
          existingSlugs={contexts.map((c) => c.slug)}
          onClose={() => setCreating(null)}
          onCreated={(slug) => {
            setCreating(null);
            changed();
            setSelectedSlug(slug);
          }}
        />
      )}
    </div>
  );
}
