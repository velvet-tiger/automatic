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
    } catch (err: any) {
      setStatus(`Error saving key: ${err}`);
    }
  };

  const handleLoad = async () => {
    try {
      const key: string = await invoke("get_api_key", { provider });
      setApiKey(key);
      setStatus(`Loaded key for ${provider} (length: ${key.length})`);
    } catch (err: any) {
      setStatus(`Error loading key (maybe not set?): ${err}`);
      setApiKey("");
    }
  };

  return (
    <div className="max-w-md bg-white dark:bg-gray-800 p-6 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700">
      <h3 className="font-semibold text-lg mb-4 text-gray-800 dark:text-gray-100">LLM Provider Config</h3>
      
      <form onSubmit={handleSave} className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Provider Name
          </label>
          <select 
            value={provider}
            onChange={(e) => setProvider(e.target.value)}
            className="w-full bg-gray-50 dark:bg-gray-700 border border-gray-300 dark:border-gray-600 rounded-md p-2 text-gray-900 dark:text-gray-100"
          >
            <option value="OpenAI">OpenAI</option>
            <option value="Anthropic">Anthropic</option>
            <option value="Gemini">Gemini</option>
            <option value="Local">Local (Ollama, etc)</option>
          </select>
        </div>

        <div>
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            API Key
          </label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-..."
            className="w-full bg-gray-50 dark:bg-gray-700 border border-gray-300 dark:border-gray-600 rounded-md p-2 text-gray-900 dark:text-gray-100"
          />
        </div>

        <div className="flex gap-2 pt-2">
          <button 
            type="submit"
            className="flex-1 bg-blue-600 hover:bg-blue-700 text-white py-2 px-4 rounded-md transition-colors"
          >
            Save Securely
          </button>
          <button 
            type="button"
            onClick={handleLoad}
            className="flex-1 bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-800 dark:text-gray-200 py-2 px-4 rounded-md transition-colors"
          >
            Load
          </button>
        </div>
      </form>

      {status && (
        <div className="mt-4 p-3 bg-gray-100 dark:bg-gray-700 rounded text-sm text-gray-700 dark:text-gray-300">
          {status}
        </div>
      )}
    </div>
  );
}
