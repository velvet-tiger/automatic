import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Eye, FileText, Loader2, RefreshCw } from "lucide-react";
import type { SourceListing } from "./types";

/**
 * Show what an agent would see when it reads this source: the entry list,
 * then one entry's text. Runs the same backend readers as the MCP tools, so
 * path, size and sign-in problems surface here first.
 */
export function SourcePreview({ contextSlug, sourceId }: { contextSlug: string; sourceId: string }) {
  const [listing, setListing] = useState<SourceListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [content, setContent] = useState<string | null>(null);
  const [reading, setReading] = useState(false);

  const load = async () => {
    setLoading(true);
    setError(null);
    setActivePath(null);
    setContent(null);
    try {
      const result: SourceListing = await invoke("list_context_source_entries", {
        slug: contextSlug,
        sourceId,
      });
      setListing(result);
    } catch (err) {
      setListing(null);
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const read = async (path: string) => {
    setActivePath(path);
    setReading(true);
    setError(null);
    try {
      const text: string = await invoke("read_context_source_entry", {
        slug: contextSlug,
        sourceId,
        path,
      });
      setContent(text);
    } catch (err) {
      setContent(null);
      setError(String(err));
    } finally {
      setReading(false);
    }
  };

  if (!listing && !error) {
    return (
      <button
        onClick={load}
        disabled={loading}
        className="flex items-center gap-1.5 text-[12px] text-brand hover:text-brand-hover font-medium transition-colors disabled:opacity-50"
      >
        {loading ? <Loader2 size={12} className="animate-spin" /> : <Eye size={12} />}
        Preview what agents see
      </button>
    );
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-medium text-text-muted">
          {listing
            ? `${listing.entries.length} entr${listing.entries.length === 1 ? "y" : "ies"}${listing.truncated ? " (list truncated)" : ""}`
            : "Preview"}
        </span>
        <button
          onClick={load}
          disabled={loading}
          className="flex items-center gap-1 text-[11px] text-text-muted hover:text-text-base transition-colors disabled:opacity-50"
        >
          <RefreshCw size={11} className={loading ? "animate-spin" : ""} /> Refresh
        </button>
      </div>

      {error && (
        <div className="flex items-start gap-2 px-2.5 py-1.5 bg-danger/10 border border-danger/30 rounded text-[11px] text-danger">
          <AlertTriangle size={12} className="flex-shrink-0 mt-0.5" />
          <span className="break-all">{error}</span>
        </div>
      )}

      {listing && listing.entries.length === 0 && (
        <p className="text-[11px] text-text-muted italic">No entries match this source.</p>
      )}

      {listing && listing.entries.length > 0 && (
        <div className="grid grid-cols-[minmax(0,14rem)_1fr] gap-2 h-56">
          <ul className="overflow-y-auto custom-scrollbar border border-border-strong/30 rounded bg-bg-base">
            {listing.entries.map((entry) => (
              <li key={entry.path}>
                <button
                  onClick={() => read(entry.path)}
                  className={`w-full flex items-center gap-1.5 px-2 py-1 text-left text-[11px] transition-colors ${
                    activePath === entry.path ? "bg-brand/15 text-text-base" : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
                  }`}
                  title={entry.path}
                >
                  <FileText size={11} className="flex-shrink-0" />
                  <span className="truncate">{entry.title || entry.path}</span>
                </button>
              </li>
            ))}
          </ul>
          <div className="overflow-auto custom-scrollbar border border-border-strong/30 rounded bg-bg-base p-2">
            {reading ? (
              <Loader2 size={14} className="animate-spin text-text-muted" />
            ) : content !== null ? (
              <pre className="text-[11px] text-text-base whitespace-pre-wrap break-words font-mono">{content}</pre>
            ) : (
              <p className="text-[11px] text-text-muted italic">Select an entry to read it.</p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
