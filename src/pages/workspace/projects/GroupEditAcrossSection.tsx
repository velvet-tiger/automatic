import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingSwitch } from "./editor/SettingSwitch";
import { supportedAgentsText } from "./writableFolders";

interface GroupEditAcrossSectionProps {
  groupName: string;
  enabled: boolean;
  /** Called after the switch changed, so the page can reload the group. */
  onChanged: () => void;
}

/**
 * "Allow agents to edit across this group". Takes effect at once, outside the
 * group's own save: the backend stores it and re-syncs every member's
 * folders in the background.
 */
export function GroupEditAcrossSection({ groupName, enabled, onChanged }: GroupEditAcrossSectionProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Shown at once, so a quick second click flips the new value, not the
  // value from before the group reloads.
  const [shown, setShown] = useState(enabled);
  useEffect(() => setShown(enabled), [enabled]);

  const toggle = async () => {
    const next = !shown;
    setBusy(true);
    setError(null);
    setShown(next);
    try {
      await invoke("set_group_agents_edit_across_group", { groupName, enabled: next });
      onChanged();
    } catch (err) {
      setShown(!next);
      setError(`Could not change "Allow agents to edit across this group": ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
      <div className="px-4 py-3 flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div className="text-[13px] font-medium text-text-base">Allow agents to edit across this group</div>
          <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
            Agents in any project in this group can edit every other project in it. Applies on this computer.
          </div>
          <div className="text-[10px] text-text-muted opacity-75 mt-1">{supportedAgentsText()}</div>
          {error && <p className="mt-1 text-[11px] text-danger">{error}</p>}
        </div>
        <SettingSwitch
          checked={shown}
          disabled={busy}
          label="Allow agents to edit across this group"
          onToggle={() => void toggle()}
        />
      </div>
    </div>
  );
}
