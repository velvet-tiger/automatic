import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function Settings() {
  const [skillSyncMode, setSkillSyncMode] = useState<string>("symlink");
  const [loading, setLoading] = useState(true);

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
  }, []);

  async function updateSetting(mode: string) {
    setSkillSyncMode(mode);
    try {
      await invoke("write_settings", { settings: { skill_sync_mode: mode } });
    } catch (e) {
      console.error("Failed to write settings", e);
    }
  }

  if (loading) {
    return <div className="flex-1 p-8 bg-[#222327]">Loading...</div>;
  }

  return (
    <div className="flex-1 p-8 bg-[#222327] overflow-y-auto h-full text-[#E0E2E8]">
      <div className="max-w-3xl">
        <h2 className="text-xl font-medium mb-6">Settings</h2>
        
        <div className="mb-8">
          <h3 className="text-sm font-medium mb-2 text-white">Skill Sync Mode</h3>
          <p className="text-[13px] text-[#C8CAD0] mb-4 leading-relaxed">
            Choose how skills are applied to your project agent directories. 
            Symlinking ensures updates to skills are immediately reflected without needing a re-sync, 
            while copying physically duplicates the file.
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
              <div className="text-[13px] font-medium text-white">Symlink (Recommended)</div>
              <div className="text-[12px] text-[#C8CAD0]">Creates a reference to the global skill file. Updates apply instantly.</div>
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
              <div className="text-[12px] text-[#C8CAD0]">Creates an independent physical copy of the skill file in the project.</div>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
