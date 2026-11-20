import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Project } from "../../types";
import { NOTICE_PRIMARY_BUTTON, NOTICE_SECONDARY_BUTTON, NoticeRow } from "./NoticeRow";

interface ProjectSummaryRow {
  local_key: string;
  id: string;
  name: string;
  directory: string;
}

interface PendingProjectMcpServersNoticeProps {
  project: Project;
  /** local_key of the open project. */
  selectedKey: string;
  /** Project servers limited to this project's id that this checkout has not accepted. */
  names: string[];
  readOnly: boolean;
  reloadProject: (projectKey: string) => Promise<void>;
  /** Sync the open project, so accepted servers reach its agent files. */
  onSync: () => void;
}

/**
 * Asks whether this checkout may use the MCP servers limited to its project
 * id. The id is committed, so another repository can copy it. The other
 * folders with the same id are listed, so the user can tell a second clone
 * of their project from a copy.
 */
export function PendingProjectMcpServersNotice({
  project,
  selectedKey,
  names,
  readOnly,
  reloadProject,
  onSync,
}: PendingProjectMcpServersNoticeProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [others, setOthers] = useState<ProjectSummaryRow[]>([]);
  const projectId = project.id ?? "";

  useEffect(() => {
    if (!projectId) return;
    let cancelled = false;
    invoke<ProjectSummaryRow[]>("get_project_summaries")
      .then((summaries) => {
        if (cancelled) return;
        setOthers(summaries.filter((s) => s.id === projectId && s.local_key !== selectedKey));
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(`Could not list the other folders with this project id: ${err}`);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, selectedKey]);

  if (!projectId || names.length === 0) return null;

  const act = async (verb: "accept" | "ignore") => {
    setBusy(true);
    setError(null);
    try {
      const command = verb === "accept" ? "confirm_project_mcp_servers" : "ignore_project_mcp_servers";
      await invoke(command, { projectName: selectedKey, projectId });
      await reloadProject(selectedKey);
      if (verb === "accept" && !readOnly) onSync();
    } catch (err) {
      setError(`Could not ${verb} this project's MCP servers: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const serverList = names.map((name) => `"${name}"`).join(", ");

  return (
    <div>
      {error && <p className="px-6 py-1.5 text-[12px] text-danger">{error}</p>}
      <NoticeRow
        severity="warning"
        title={`This project id has MCP servers that this folder does not use yet: ${serverList}.`}
        actions={
          <>
            <button onClick={() => void act("accept")} disabled={busy} className={NOTICE_PRIMARY_BUTTON}>
              Accept
            </button>
            <button onClick={() => void act("ignore")} disabled={busy} className={NOTICE_SECONDARY_BUTTON}>
              Ignore
            </button>
          </>
        }
      >
        <div>
          They can hold secrets. Accept only if this folder is the same project as the one that set them up.
          Another repository can copy a project id.
        </div>
        {others.length > 0 && (
          <div className="mt-1">
            Other folders with this project id:
            <ul className="mt-0.5">
              {others.map((other) => (
                <li key={other.local_key || other.name} className="font-mono truncate">
                  {other.name}: {other.directory || "no folder"}
                </li>
              ))}
            </ul>
          </div>
        )}
      </NoticeRow>
    </div>
  );
}
