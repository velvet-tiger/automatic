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
          <p className="text-[13px] text-[#8A8C93] mb-4 leading-relaxed">
            Choose how skills are applied to your project agent directories. 
            Symlinking ensures updates to skills are immediately reflected without needing a re-sync, 
            while copying physically duplicates the file.
          </p>
          
          <div className="flex flex-col gap-3">
            <label className="flex items-start gap-3 cursor-pointer group">
              <input 
                type="radio" 
                name="skill_sync_mode" 
                value="symlink" 
                checked={skillSyncMode === "symlink"}
                onChange={() => updateSetting("symlink")}
                className="mt-1 w-4 h-4 bg-[#18191C] border-[#3E4048] rounded-full checked:bg-[#5E6AD2] focus:ring-[#5E6AD2] focus:ring-offset-[#222327]"
              />
              <div>
                <div className="text-[13px] font-medium text-[#E0E2E8] group-hover:text-white transition-colors">Symlink (Recommended)</div>
                <div className="text-[12px] text-[#8A8C93]">Creates a reference to the global skill file. Updates apply instantly.</div>
              </div>
            </label>
            
            <label className="flex items-start gap-3 cursor-pointer group">
              <input 
                type="radio" 
                name="skill_sync_mode" 
                value="copy" 
                checked={skillSyncMode === "copy"}
                onChange={() => updateSetting("copy")}
                className="mt-1 w-4 h-4 bg-[#18191C] border-[#3E4048] rounded-full checked:bg-[#5E6AD2] focus:ring-[#5E6AD2] focus:ring-offset-[#222327]"
              />
              <div>
                <div className="text-[13px] font-medium text-[#E0E2E8] group-hover:text-white transition-colors">Copy</div>
                <div className="text-[12px] text-[#8A8C93]">Creates an independent physical copy of the skill file in the project.</div>
              </div>
            </label>
          </div>
        </div>
      </div>
    </div>
  );
}
