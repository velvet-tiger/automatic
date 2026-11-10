import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { SettingSwitch } from "../workspace/projects/editor/SettingSwitch";

/** Where Automatic saves the folders Claude Code may edit. Matches `WritableFoldersTarget`. */
export type WritableFoldersTarget = "prefer_local" | "always_user";

interface FolderSettings {
  target: WritableFoldersTarget;
  consent: boolean;
}

/** The two fields this section shows, from `read_settings`. Anything unexpected reads as the default. */
export function parseFolderSettings(raw: Record<string, unknown>): FolderSettings {
  return {
    target: raw.writable_folders_target === "always_user" ? "always_user" : "prefer_local",
    consent: raw.writable_folders_user_consent === true,
  };
}

const TARGETS: { value: WritableFoldersTarget; label: string; description: string }[] = [
  {
    value: "prefer_local",
    label: "In each project, when possible",
    description:
      "Only that project's sessions get its folders. Projects whose own settings can't be used ask you first.",
  },
  {
    value: "always_user",
    label: "For all my Claude Code sessions",
    description:
      "Every session on this computer can edit every folder you have allowed. Suits you if you commit Claude Code's settings in most projects.",
  },
];

/**
 * Settings → Sync: where the folders Claude Code may edit are saved, and
 * whether Automatic may change Claude Code's user settings. Each change is
 * stored at once, and the backend then refreshes every project.
 */
export function ClaudeFoldersSettings() {
  const [current, setCurrent] = useState<FolderSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    const raw: Record<string, unknown> = await invoke("read_settings");
    setCurrent(parseFolderSettings(raw));
  };

  useEffect(() => {
    load().catch((err: unknown) => setError(`Could not read these settings: ${err}`));
  }, []);

  const change = async (what: string, step: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await step();
      await load();
    } catch (err) {
      setError(`Could not ${what}: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const chooseTarget = (target: WritableFoldersTarget) =>
    change("change where the folders are saved", () => invoke("set_writable_folders_target", { target }));

  // Withdrawing removes Automatic's folders from every session at once, so it
  // asks first. The backend also moves the choice back to "In each project".
  const toggleConsent = async () => {
    if (!current) return;
    if (current.consent) {
      const confirmed = await ask(
        "Automatic will remove the folders it saved for all your Claude Code sessions. Projects whose own settings can't be used will ask you again.",
        { title: "Remove the folders Automatic saved for all your sessions?", kind: "warning", okLabel: "Remove", cancelLabel: "Cancel" },
      );
      if (!confirmed) return;
    }
    await change("change this setting", () => invoke("set_writable_folders_user_consent", { consent: !current.consent }));
  };

  return (
    <div className="mb-8">
      <h3 className="text-sm font-medium mb-2 text-text-base">Folders Claude Code Can Edit</h3>
      <p className="text-[13px] text-text-muted mb-4 leading-relaxed">
        Where Automatic saves the folders each project&apos;s Claude Code may edit.
      </p>
      {error && (
        <div className="mb-3 p-3 rounded-lg border border-danger bg-danger/10 text-[13px] text-danger">{error}</div>
      )}
      {current && (
        <>
          <div className="flex flex-col gap-2" role="radiogroup" aria-label="Where to save the folders">
            {TARGETS.map((option) => (
              <button
                key={option.value}
                role="radio"
                aria-checked={current.target === option.value}
                disabled={busy}
                onClick={() => {
                  if (current.target !== option.value) void chooseTarget(option.value);
                }}
                className={`flex flex-col items-start gap-1 p-4 rounded-lg border text-left transition-all disabled:opacity-60 ${
                  current.target === option.value
                    ? "border-brand bg-brand/10"
                    : "border-border-strong/40 bg-bg-input-dark hover:border-border-strong hover:bg-surface-hover"
                }`}
              >
                <div className="text-[13px] font-medium text-text-base">{option.label}</div>
                <div className="text-[12px] text-text-muted">{option.description}</div>
              </button>
            ))}
          </div>
          <div className="mt-3 p-4 rounded-lg border border-border-strong/40 bg-bg-input-dark flex items-center justify-between gap-4">
            <div className="min-w-0">
              <div className="text-[13px] font-medium text-text-base">Let Automatic change my Claude Code settings</div>
              <div className="text-[12px] text-text-muted mt-0.5 leading-relaxed">
                Needed for a project whose own settings can&apos;t be used, for example a read-only or Silent project.
                Automatic only adds and removes the folders it saved. It never changes anything else.
              </div>
            </div>
            <SettingSwitch
              checked={current.consent}
              disabled={busy}
              label="Let Automatic change my Claude Code settings"
              onToggle={() => void toggleConsent()}
            />
          </div>
        </>
      )}
    </div>
  );
}
