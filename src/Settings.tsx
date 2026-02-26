import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";

type UpdateStatus =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "downloading"
  | "installed"
  | "error";

export default function Settings() {
  const [skillSyncMode, setSkillSyncMode] = useState<string>("symlink");
  const [loading, setLoading] = useState(true);

  // Update state
  const [appVersion, setAppVersion] = useState<string>("");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>("idle");
  const [pendingUpdate, setPendingUpdate] = useState<Update | null>(null);
  const [updateInfo, setUpdateInfo] = useState<{
    version: string;
    notes?: string;
  } | null>(null);
  const [updateError, setUpdateError] = useState<string>("");

  useEffect(() => {
    async function loadSettings() {
      try {
        const settings: any = await invoke("read_settings");
        setSkillSyncMode(settings.skill_sync_mode);
      } catch (e) {
        console.error("Failed to read settings", e);
      } finally {
        setLoading(false);
      }
    }
    loadSettings();
    getVersion()
      .then(setAppVersion)
      .catch(() => {});
  }, []);

  async function updateSetting(mode: string) {
    setSkillSyncMode(mode);
    try {
      await invoke("write_settings", { settings: { skill_sync_mode: mode } });
    } catch (e) {
      console.error("Failed to write settings", e);
    }
  }

  async function checkForUpdates() {
    setUpdateStatus("checking");
    setUpdateError("");
    setPendingUpdate(null);
    setUpdateInfo(null);
    try {
      const update = await check();
      if (update) {
        setPendingUpdate(update);
        setUpdateInfo({
          version: update.version,
          notes: update.body ?? undefined,
        });
        setUpdateStatus("available");
      } else {
        setUpdateStatus("up-to-date");
      }
    } catch (e) {
      setUpdateError(String(e));
      setUpdateStatus("error");
    }
  }

  async function installUpdate() {
    if (!pendingUpdate) return;
    setUpdateStatus("downloading");
    try {
      await pendingUpdate.downloadAndInstall();
      setUpdateStatus("installed");
    } catch (e) {
      setUpdateError(String(e));
      setUpdateStatus("error");
    }
  }

  function restartApp() {
    invoke("restart_app");
  }

  if (loading) {
    return <div className="flex-1 p-8 bg-[#222327]">Loading...</div>;
  }

  return (
    <div className="flex-1 p-8 bg-[#222327] overflow-y-auto h-full text-[#E0E2E8]">
      <div className="max-w-3xl">
        <h2 className="text-xl font-medium mb-6">Settings</h2>

        {/* ── Skill Sync Mode ─────────────────────────────────────── */}
        <div className="mb-8">
          <h3 className="text-sm font-medium mb-2 text-white">Skill Sync Mode</h3>
          <p className="text-[13px] text-[#C8CAD0] mb-4 leading-relaxed">
            Choose how skills are applied to your project agent directories.
            Symlinking ensures updates to skills are immediately reflected
            without needing a re-sync, while copying physically duplicates the
            file.
          </p>

          <div className="flex flex-col gap-2">
            <button
              onClick={() => updateSetting("symlink")}
              className={`flex flex-col items-start gap-1 p-4 rounded-lg border text-left transition-all ${
                skillSyncMode === "symlink"
                  ? "border-[#5E6AD2] bg-[#5E6AD2]/10"
                  : "border-[#3E4048] bg-[#18191C] hover:border-[#5E5E6A] hover:bg-[#1E1F24]"
              }`}
            >
              <div className="text-[13px] font-medium text-white">
                Symlink (Recommended)
              </div>
              <div className="text-[12px] text-[#C8CAD0]">
                Creates a reference to the global skill file. Updates apply
                instantly.
              </div>
            </button>

            <button
              onClick={() => updateSetting("copy")}
              className={`flex flex-col items-start gap-1 p-4 rounded-lg border text-left transition-all ${
                skillSyncMode === "copy"
                  ? "border-[#5E6AD2] bg-[#5E6AD2]/10"
                  : "border-[#3E4048] bg-[#18191C] hover:border-[#5E5E6A] hover:bg-[#1E1F24]"
              }`}
            >
              <div className="text-[13px] font-medium text-white">Copy</div>
              <div className="text-[12px] text-[#C8CAD0]">
                Creates an independent physical copy of the skill file in the
                project.
              </div>
            </button>
          </div>
        </div>

        {/* ── App Updates ─────────────────────────────────────────── */}
        <div className="mb-8">
          <h3 className="text-sm font-medium mb-2 text-white">App Updates</h3>

          <div className="flex items-center gap-3 mb-4">
            {appVersion && (
              <span className="text-[12px] text-[#C8CAD0]">
                Version {appVersion}
              </span>
            )}
            {updateStatus === "up-to-date" && (
              <span className="text-[12px] text-[#4CAF50]">Up to date</span>
            )}
          </div>

          {/* Update available banner */}
          {updateStatus === "available" && updateInfo && (
            <div className="mb-4 p-4 rounded-lg border border-[#5E6AD2] bg-[#5E6AD2]/10">
              <div className="text-[13px] font-medium text-white mb-1">
                Version {updateInfo.version} available
              </div>
              {updateInfo.notes && (
                <p className="text-[12px] text-[#C8CAD0] mb-3 leading-relaxed whitespace-pre-wrap">
                  {updateInfo.notes}
                </p>
              )}
              <button
                onClick={installUpdate}
                className="px-3 py-1.5 rounded text-[12px] font-medium bg-[#5E6AD2] text-white hover:bg-[#4E5AC2] transition-colors"
              >
                Download &amp; Install
              </button>
            </div>
          )}

          {/* Downloading */}
          {updateStatus === "downloading" && (
            <div className="mb-4 p-4 rounded-lg border border-[#3E4048] bg-[#18191C] text-[13px] text-[#C8CAD0]">
              Downloading update...
            </div>
          )}

          {/* Installed — prompt restart */}
          {updateStatus === "installed" && (
            <div className="mb-4 p-4 rounded-lg border border-[#4CAF50] bg-[#4CAF50]/10">
              <div className="text-[13px] font-medium text-white mb-1">
                Update installed
              </div>
              <p className="text-[12px] text-[#C8CAD0] mb-3">
                Restart Automatic to apply the update.
              </p>
              <button
                onClick={restartApp}
                className="px-3 py-1.5 rounded text-[12px] font-medium bg-[#4CAF50] text-white hover:bg-[#3D9F40] transition-colors"
              >
                Restart Now
              </button>
            </div>
          )}

          {/* Error */}
          {updateStatus === "error" && updateError && (
            <div className="mb-4 p-4 rounded-lg border border-[#E05252] bg-[#E05252]/10 text-[13px] text-[#E05252]">
              {updateError}
            </div>
          )}

          {/* Check button — hidden while update is downloaded/installed */}
          {updateStatus !== "downloading" && updateStatus !== "installed" && (
            <button
              onClick={checkForUpdates}
              disabled={updateStatus === "checking"}
              className="px-4 py-2 rounded-lg border border-[#3E4048] bg-[#18191C] text-[13px] text-[#E0E2E8] hover:border-[#5E5E6A] hover:bg-[#1E1F24] transition-all disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {updateStatus === "checking" ? "Checking..." : "Check for Updates"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
