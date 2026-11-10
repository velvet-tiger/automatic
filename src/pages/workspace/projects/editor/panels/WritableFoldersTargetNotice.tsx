import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UserSettingsReason, WritableFoldersTargetNotice as TargetNotice } from "../../types";
import { NOTICE_PRIMARY_BUTTON, NOTICE_SECONDARY_BUTTON, NoticeRow } from "./NoticeRow";

/** Why this project's own Claude Code settings can't hold its folders, ending a sentence. */
export function targetReasonText(reason: UserSettingsReason): string {
  switch (reason) {
    case "preferred":
      return "because your settings say to save them for all your sessions.";
    case "silent":
      return "because Silent mode never writes into the project folder.";
    case "read_only":
      return "because this project is read only on this computer.";
    case "tracked":
      return "because they are committed to git.";
    case "seen_tracked":
      return (
        "because Claude Code stopped trusting them after they were once committed. " +
        "Open Claude Code here and accept its trust prompt to use them again."
      );
    case "not_repository_root":
      return "because this folder is not the top of a git repository.";
    case "git_unavailable":
    case "trust_unknown":
      return "because Automatic could not check them.";
  }
}

interface WritableFoldersTargetNoticeProps {
  notice: TargetNotice;
  /** local_key of the open project. */
  selectedKey: string;
  reloadProject: (projectKey: string) => Promise<void>;
  refreshProblems: () => Promise<void>;
}

/**
 * Asks whether Automatic may save this project's folders in Claude Code's
 * user settings, because its own settings can't hold them. "Save for all my
 * sessions" agrees for every project. "Not now" hides it for this project.
 */
export function WritableFoldersTargetNotice({
  notice,
  selectedKey,
  reloadProject,
  refreshProblems,
}: WritableFoldersTargetNoticeProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (what: string, step: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await step();
    } catch (err) {
      setError(`Could not ${what}: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  // Consent queues a refresh of every project. Syncing this one now means the
  // notice does not turn into a "cannot edit" problem while that runs.
  const save = () =>
    run("save the folders for all your sessions", async () => {
      await invoke("set_writable_folders_user_consent", { consent: true });
      await invoke("sync_project_writable_folders", { name: selectedKey });
      await refreshProblems();
    });

  const notNow = () =>
    run("hide this notice", async () => {
      await invoke("set_writable_folders_notice_hidden", { name: selectedKey, hidden: true });
      await reloadProject(selectedKey);
      await refreshProblems();
    });

  const count = notice.folders.length;
  return (
    <div>
      {error && <p className="px-6 py-1.5 text-[12px] text-danger">{error}</p>}
      <NoticeRow
        severity="warning"
        title="Claude Code needs your OK to save this project's folders"
        actions={
          <>
            <button onClick={() => void save()} disabled={busy} className={NOTICE_PRIMARY_BUTTON}>
              Save for all my sessions
            </button>
            <button onClick={() => void notNow()} disabled={busy} className={NOTICE_SECONDARY_BUTTON}>
              Not now
            </button>
          </>
        }
      >
        This project&apos;s own Claude Code settings can&apos;t be used, {targetReasonText(notice.reason)}{" "}
        Automatic can save {count === 1 ? "the folder" : `the ${count} folders`} for all your Claude Code
        sessions instead. Every session on this computer could then edit {count === 1 ? "it" : "them"}.
      </NoticeRow>
    </div>
  );
}
