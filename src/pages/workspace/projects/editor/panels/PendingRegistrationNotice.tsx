import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { NOTICE_PRIMARY_BUTTON, NOTICE_SECONDARY_BUTTON, NoticeRow } from "./NoticeRow";

interface PendingRegistrationNoticeProps {
  projectName: string;
  /** local_key of the open project. */
  selectedKey: string;
  reloadProject: (projectKey: string) => Promise<void>;
  /** The normal project removal, with its own confirmation. */
  onRemove: () => Promise<void>;
}

/**
 * Asks about a project an agent registered. Until the user keeps it, agents
 * cannot read its contexts or details, and it is nobody's peer.
 */
export function PendingRegistrationNotice({
  projectName, selectedKey, reloadProject, onRemove,
}: PendingRegistrationNoticeProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (verb: "keep" | "remove", action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (err) {
      setError(`Could not ${verb} "${projectName}": ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const keep = async () => {
    await invoke("confirm_project_registration", { projectName: selectedKey });
    await reloadProject(selectedKey);
  };

  return (
    <div>
      {error && <p className="px-6 py-1.5 text-[12px] text-danger">{error}</p>}
      <NoticeRow
        severity="warning"
        title="An agent added this project."
        actions={
          <>
            <button onClick={() => void run("keep", keep)} disabled={busy} className={NOTICE_PRIMARY_BUTTON}>
              Keep
            </button>
            <button onClick={() => void run("remove", onRemove)} disabled={busy} className={NOTICE_SECONDARY_BUTTON}>
              Remove
            </button>
          </>
        }
      >
        Agents cannot read its contexts or project details until you keep it.
      </NoticeRow>
    </div>
  );
}
