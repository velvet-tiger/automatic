import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ShieldAlert } from "lucide-react";

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
    <div className="space-y-2">
      {error && <p className="text-[12px] text-danger">{error}</p>}
      <div className="flex items-start gap-3 px-3 py-2.5 bg-warning/10 border border-warning/30 rounded-lg">
        <ShieldAlert size={14} className="flex-shrink-0 mt-0.5 text-warning" />
        <div className="flex-1 min-w-0">
          <p className="text-[13px] font-medium text-text-base">An agent added this project.</p>
          <p className="text-[12px] text-text-muted mt-0.5">
            Agents cannot read its contexts or project details until you keep it.
          </p>
        </div>
        <button
          onClick={() => void run("keep", keep)}
          disabled={busy}
          className="px-2.5 py-1 text-[12px] font-medium rounded bg-brand/15 text-brand border border-brand/30 hover:bg-brand/25 transition-colors disabled:opacity-50 flex-shrink-0"
        >
          Keep
        </button>
        <button
          onClick={() => void run("remove", onRemove)}
          disabled={busy}
          className="px-2.5 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors disabled:opacity-50 flex-shrink-0"
        >
          Remove
        </button>
      </div>
    </div>
  );
}
