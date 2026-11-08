import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Project } from "../../types";
import type { ContextTarget } from "../../../contexts/types";
import { NOTICE_PRIMARY_BUTTON, NOTICE_SECONDARY_BUTTON, NoticeRow } from "./NoticeRow";

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
    <div>
      {error && <p className="px-6 py-1.5 text-[12px] text-danger">{error}</p>}
      {waiting.map((slug) => (
        <NoticeRow
          key={slug}
          severity="warning"
          title={`This project's shared settings include the context "${slug}".`}
          actions={
            <>
              <button onClick={() => void act("attach", slug)} disabled={busy} className={NOTICE_PRIMARY_BUTTON}>
                Attach
              </button>
              <button onClick={() => void act("ignore", slug)} disabled={busy} className={NOTICE_SECONDARY_BUTTON}>
                Ignore
              </button>
            </>
          }
        >
          It is not turned on for this computer yet. Agents here cannot read it until you attach it.
        </NoticeRow>
      ))}
    </div>
  );
}
