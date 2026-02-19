import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function Providers() {
  const [provider, setProvider] = useState("OpenAI");
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState("");

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await invoke("save_api_key", { provider, key: apiKey });
      setStatus(`Successfully saved key for ${provider}`);
      setApiKey(""); // clear after save
      setTimeout(() => setStatus(""), 3000);
    } catch (err: any) {
      setStatus(`Error saving key: ${err}`);
    }
  };

  const handleLoad = async () => {
    try {
      const key: string = await invoke("get_api_key", { provider });
      setApiKey(key);
      setStatus(`Loaded key for ${provider}`);
      setTimeout(() => setStatus(""), 3000);
    } catch (err: any) {
      setStatus(`Error loading key (maybe not set?): ${err}`);
      setApiKey("");
    }
  };

  return (
    <div className="w-full">
      <div className="mb-8">
        <h2 className="text-lg font-medium text-[#E0E1E6] mb-2">LLM Providers</h2>
        <p className="text-[14px] text-[#8A8C93] leading-relaxed">
          Configure API keys for external Language Models. Keys are securely stored in your native OS keychain and never saved in plain text.
        </p>
      </div>

      <div className="bg-[#1A1A1E] border border-[#33353A] rounded-lg overflow-hidden">
        <div className="p-6 border-b border-[#33353A]">
          <form onSubmit={handleSave} className="space-y-6">
            <div>
              <label className="block text-[13px] font-medium text-[#E0E1E6] mb-2">
                Provider Name
              </label>
              <select 
                value={provider}
                onChange={(e) => setProvider(e.target.value)}
                className="w-full bg-[#222327] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] focus:ring-1 focus:ring-[#5E6AD2] outline-none rounded-md px-3 py-2 text-[13px] text-[#E0E1E6] transition-colors"
              >
                <option value="OpenAI">OpenAI</option>
                <option value="Anthropic">Anthropic</option>
                <option value="Gemini">Gemini</option>
                <option value="Local">Local (Ollama, etc)</option>
              </select>
            </div>

            <div>
              <label className="block text-[13px] font-medium text-[#E0E1E6] mb-2">
                API Key
              </label>
              <div className="flex gap-2">
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-..."
                  className="flex-1 bg-[#222327] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] focus:ring-1 focus:ring-[#5E6AD2] outline-none rounded-md px-3 py-2 text-[13px] text-[#E0E1E6] transition-colors font-mono"
                />
                <button 
                  type="button"
                  onClick={handleLoad}
                  className="px-4 py-2 bg-[#2D2E36] hover:bg-[#33353A] text-[#E0E1E6] text-[13px] font-medium rounded-md border border-[#3A3B42] transition-colors"
                >
                  Load existing
                </button>
              </div>
            </div>

            <div className="flex items-center gap-4 pt-2">
              <button 
                type="submit"
                disabled={!apiKey}
                className="px-4 py-2 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded-md shadow-sm transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Save securely
              </button>
              
              {status && (
                <span className="text-[13px] text-[#8A8C93] animate-fade-in">
                  {status}
                </span>
              )}
            </div>
          </form>
        </div>
        <div className="bg-[#222327]/50 px-6 py-4 text-[12px] text-[#8A8C93] flex items-center gap-2">
          <span className="w-2 h-2 rounded-full bg-green-500/80"></span>
          System keychain access is active and securing your credentials.
        </div>
      </div>
    </div>
  );
}
