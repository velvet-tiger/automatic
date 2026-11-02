import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BookMarked } from "lucide-react";
import type { Project } from "../../types";
import type { ContextTarget } from "../../../contexts/types";

interface PendingContextsNoticeProps {
  project: Project;
  /** local_key of the open project. */
  selectedKey: string;
  reloadProject: (projectKey: string) => Promise<void>;
}

/**
 * Asks about contexts the project's committed config names but this checkout
 * has not accepted. Agents cannot read them until the user clicks Attach.
 */
export function PendingContextsNotice({ project, selectedKey, reloadProject }: PendingContextsNoticeProps) {
  const ignored = project.local_ignored_contexts ?? [];
  const waiting = (project.pending_contexts ?? []).filter((slug) => !ignored.includes(slug));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (waiting.length === 0) return null;

  const act = async (verb: "attach" | "ignore", slug: string) => {
    setBusy(true);
    setError(null);
    try {
      if (verb === "attach") {
        const target: ContextTarget = { type: "project", name: selectedKey };
        await invoke("attach_context", { target, slug });
      } else {
        await invoke("ignore_project_context", { projectName: selectedKey, slug });
      }
      await reloadProject(selectedKey);
    } catch (err) {
      setError(`Could not ${verb} the context "${slug}": ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-2">
      {error && <p className="text-[12px] text-danger">{error}</p>}
      {waiting.map((slug) => (
        <div
          key={slug}
          className="flex items-start gap-3 px-3 py-2.5 bg-warning/10 border border-warning/30 rounded-lg"
        >
          <BookMarked size={14} className="flex-shrink-0 mt-0.5 text-warning" />
          <div className="flex-1 min-w-0">
            <p className="text-[13px] font-medium text-text-base">This repository asks for the context "{slug}".</p>
            <p className="text-[12px] text-text-muted mt-0.5">
              Agents in this project cannot read it until you attach it.
            </p>
          </div>
          <button
            onClick={() => void act("attach", slug)}
            disabled={busy}
            className="px-2.5 py-1 text-[12px] font-medium rounded bg-brand/15 text-brand border border-brand/30 hover:bg-brand/25 transition-colors disabled:opacity-50 flex-shrink-0"
          >
            Attach
          </button>
          <button
            onClick={() => void act("ignore", slug)}
            disabled={busy}
            className="px-2.5 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors disabled:opacity-50 flex-shrink-0"
          >
            Ignore
          </button>
        </div>
      ))}
    </div>
  );
}
