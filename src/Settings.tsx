import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import {
  setAnalyticsEnabled,
  trackSettingChanged,
  trackUpdateChecked,
  trackUpdateInstalled,
} from "./analytics";
import { AgentSelector, type AgentInfo } from "./AgentSelector";
import { Code2, Bot, AppWindow } from "lucide-react";

type UpdateStatus =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "downloading"
  | "installed"
  | "error";

type SettingsPage = "skills" | "agents" | "app";

interface AppSettings {
  skill_sync_mode: string;
  analytics_enabled: boolean;
  default_agents: string[];
}

const PAGES: { id: SettingsPage; label: string; icon: React.ReactNode; description: string }[] = [
  {
    id: "skills",
    label: "Skills",
    icon: <Code2 size={15} />,
    description: "Sync mode",
  },
  {
    id: "agents",
    label: "Agents",
    icon: <Bot size={15} />,
    description: "Default agents",
  },
  {
    id: "app",
    label: "App",
    icon: <AppWindow size={15} />,
    description: "Analytics & updates",
  },
];

export default function Settings() {
  const [activePage, setActivePage] = useState<SettingsPage>("skills");
  const [settings, setSettings] = useState<AppSettings>({
    skill_sync_mode: "symlink",
    analytics_enabled: true,
    default_agents: [],
  });
  const [loading, setLoading] = useState(true);
  const [availableAgents, setAvailableAgents] = useState<AgentInfo[]>([]);

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
        const raw: any = await invoke("read_settings");
        const agents = await invoke<AgentInfo[]>("list_agents");
        setSettings({
          skill_sync_mode: raw.skill_sync_mode ?? "symlink",
          analytics_enabled: raw.analytics_enabled ?? true,
          default_agents: raw.default_agents ?? [],
        });
        setAvailableAgents(agents);
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

  async function persistSettings(updated: AppSettings) {
    try {
      await invoke("write_settings", { settings: updated });
    } catch (e) {
      console.error("Failed to write settings", e);
    }
  }

  async function updateSkillSyncMode(mode: string) {
    const updated = { ...settings, skill_sync_mode: mode };
    setSettings(updated);
    trackSettingChanged("skill_sync_mode", mode);
    await persistSettings(updated);
  }

  async function updateAnalyticsEnabled(enabled: boolean) {
    const updated = { ...settings, analytics_enabled: enabled };
    setSettings(updated);
    setAnalyticsEnabled(enabled);
    if (enabled) {
      trackSettingChanged("analytics_enabled", enabled);
    }
    await persistSettings(updated);
  }

  async function addDefaultAgent(id: string) {
    if (settings.default_agents.includes(id)) return;
    const updated = { ...settings, default_agents: [...settings.default_agents, id] };
    setSettings(updated);
    await persistSettings(updated);
  }

  async function removeDefaultAgent(index: number) {
    const updated = {
      ...settings,
      default_agents: settings.default_agents.filter((_, i) => i !== index),
    };
    setSettings(updated);
    await persistSettings(updated);
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
        trackUpdateChecked("available");
      } else {
        setUpdateStatus("up-to-date");
        trackUpdateChecked("not_available");
      }
    } catch (e) {
      setUpdateError(String(e));
      setUpdateStatus("error");
      trackUpdateChecked("error");
    }
  }

  async function installUpdate() {
    if (!pendingUpdate) return;
    setUpdateStatus("downloading");
    try {
      await pendingUpdate.downloadAndInstall();
      setUpdateStatus("installed");
      trackUpdateInstalled(pendingUpdate.version);
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
    <div className="flex flex-1 h-full bg-[#222327] overflow-hidden text-[#E0E2E8]">
      {/* Sub-page sidebar */}
      <div className="w-52 flex-shrink-0 border-r border-[#2E3038] flex flex-col py-3">
        <div className="px-3 mb-2">
          <span className="text-[11px] font-medium text-[#6B6E7A] uppercase tracking-wider">
            Settings
          </span>
        </div>
        <nav className="flex flex-col gap-0.5 px-2">
          {PAGES.map((page) => {
            const isActive = activePage === page.id;
            return (
              <button
                key={page.id}
                onClick={() => setActivePage(page.id)}
                className={`flex items-center gap-2.5 px-2.5 py-2 rounded-md text-left transition-colors w-full ${
                  isActive
                    ? "bg-[#2E3038] text-white"
                    : "text-[#C8CAD0] hover:bg-[#2A2B30] hover:text-white"
                }`}
              >
                <span className={isActive ? "text-[#5E6AD2]" : "text-[#9B9EA8]"}>
                  {page.icon}
                </span>
                <div className="flex flex-col min-w-0">
                  <span className="text-[13px] font-medium leading-tight">{page.label}</span>
                  <span className="text-[11px] text-[#9B9EA8] leading-tight truncate">
                    {page.description}
                  </span>
                </div>
              </button>
            );
          })}
        </nav>
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-y-auto h-full">
        <div className="p-8 max-w-2xl">

          {/* ── Skills page ─────────────────────────────────────────── */}
          {activePage === "skills" && (
            <div>
              <h2 className="text-lg font-medium mb-1 text-white">Skills</h2>
              <p className="text-[13px] text-[#6B6E7A] mb-6">Configure how skills are applied to your projects.</p>

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
                    onClick={() => updateSkillSyncMode("symlink")}
                    className={`flex flex-col items-start gap-1 p-4 rounded-lg border text-left transition-all ${
                      settings.skill_sync_mode === "symlink"
                        ? "border-[#5E6AD2] bg-[#5E6AD2]/10"
                        : "border-[#3E4048] bg-[#18191C] hover:border-[#5E5E6A] hover:bg-[#1E1F24]"
                    }`}
                  >
                    <div className="text-[13px] font-medium text-white">
                      Symlink (Recommended)
                    </div>
                    <div className="text-[12px] text-[#C8CAD0]">
                      Creates a reference to the global skill file. Updates apply instantly.
                    </div>
                  </button>

                  <button
                    onClick={() => updateSkillSyncMode("copy")}
                    className={`flex flex-col items-start gap-1 p-4 rounded-lg border text-left transition-all ${
                      settings.skill_sync_mode === "copy"
                        ? "border-[#5E6AD2] bg-[#5E6AD2]/10"
                        : "border-[#3E4048] bg-[#18191C] hover:border-[#5E5E6A] hover:bg-[#1E1F24]"
                    }`}
                  >
                    <div className="text-[13px] font-medium text-white">Copy</div>
                    <div className="text-[12px] text-[#C8CAD0]">
                      Creates an independent physical copy of the skill file in the project.
                    </div>
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* ── Agents page ─────────────────────────────────────────── */}
          {activePage === "agents" && (
            <div>
              <h2 className="text-lg font-medium mb-1 text-white">Agents</h2>
              <p className="text-[13px] text-[#6B6E7A] mb-6">Configure default agent behaviour for new projects.</p>

              <div className="mb-8">
                <h3 className="text-sm font-medium mb-2 text-white">Default Agents</h3>
                <p className="text-[13px] text-[#C8CAD0] mb-4 leading-relaxed">
                  These agents are automatically pre-selected when creating a new
                  project. You can add or remove agents per-project after creation.
                </p>
                <div className="p-4 rounded-lg border border-[#3E4048] bg-[#18191C]">
                  <AgentSelector
                    agentIds={settings.default_agents}
                    availableAgents={availableAgents}
                    onAdd={addDefaultAgent}
                    onRemove={removeDefaultAgent}
                    label="Default Agents"
                    emptyMessage="No default agents. New projects will start with no agents selected."
                  />
                </div>
              </div>
            </div>
          )}

          {/* ── App page ────────────────────────────────────────────── */}
          {activePage === "app" && (
            <div>
              <h2 className="text-lg font-medium mb-1 text-white">App</h2>
              <p className="text-[13px] text-[#6B6E7A] mb-6">Analytics preferences and application updates.</p>

              {/* Analytics */}
              <div className="mb-8">
                <h3 className="text-sm font-medium mb-2 text-white">Analytics</h3>
                <p className="text-[13px] text-[#C8CAD0] mb-4 leading-relaxed">
                  Help us improve Automatic by sharing anonymous usage data. No
                  personal information, file contents, or project names are ever
                  collected. Analytics are always disabled during local development.
                </p>

                <button
                  onClick={() => updateAnalyticsEnabled(!settings.analytics_enabled)}
                  className={`flex items-center justify-between w-full p-4 rounded-lg border text-left transition-all ${
                    settings.analytics_enabled
                      ? "border-[#5E6AD2] bg-[#5E6AD2]/10"
                      : "border-[#3E4048] bg-[#18191C] hover:border-[#5E5E6A] hover:bg-[#1E1F24]"
                  }`}
                >
                  <div>
                    <div className="text-[13px] font-medium text-white">
                      Anonymous usage analytics
                    </div>
                    <div className="text-[12px] text-[#C8CAD0]">
                      {settings.analytics_enabled
                        ? "Enabled — thank you for helping improve Automatic"
                        : "Disabled"}
                    </div>
                  </div>

                  {/* Toggle pill */}
                  <div
                    className={`relative flex-shrink-0 w-10 h-5 rounded-full transition-colors ${
                      settings.analytics_enabled ? "bg-[#5E6AD2]" : "bg-[#3E4048]"
                    }`}
                  >
                    <div
                      className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-all ${
                        settings.analytics_enabled ? "left-5" : "left-0.5"
                      }`}
                    />
                  </div>
                </button>
              </div>

              {/* App Updates */}
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

                {/* Check button */}
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
          )}

        </div>
      </div>
    </div>
  );
}
