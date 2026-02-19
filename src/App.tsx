import { useState } from "react";
import Providers from "./Providers";
import Skills from "./Skills";
import "./App.css";

function App() {
  const [activeTab, setActiveTab] = useState("agents");

  return (
    <div className="flex h-screen w-screen overflow-hidden text-gray-900 bg-gray-50 dark:bg-gray-900 dark:text-gray-100">
      {/* Sidebar */}
      <aside className="w-64 bg-gray-100 dark:bg-gray-800 border-r border-gray-200 dark:border-gray-700 flex flex-col">
        <div className="p-4 border-b border-gray-200 dark:border-gray-700 font-bold text-lg flex items-center gap-2">
          <span>Agentic Desktop</span>
        </div>
        <nav className="flex-1 overflow-y-auto py-4">
          <ul className="space-y-1 px-2">
            <li>
              <button
                onClick={() => setActiveTab("agents")}
                className={`w-full text-left px-3 py-2 rounded-md ${
                  activeTab === "agents" ? "bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-100 font-medium" : "hover:bg-gray-200 dark:hover:bg-gray-700"
                }`}
              >
                Local Agents
              </button>
            </li>
            <li>
              <button
                onClick={() => setActiveTab("providers")}
                className={`w-full text-left px-3 py-2 rounded-md ${
                  activeTab === "providers" ? "bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-100 font-medium" : "hover:bg-gray-200 dark:hover:bg-gray-700"
                }`}
              >
                LLM Providers
              </button>
            </li>
            <li>
              <button
                onClick={() => setActiveTab("skills")}
                className={`w-full text-left px-3 py-2 rounded-md ${
                  activeTab === "skills" ? "bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-100 font-medium" : "hover:bg-gray-200 dark:hover:bg-gray-700"
                }`}
              >
                Skills & Prompts
              </button>
            </li>
            <li>
              <button
                onClick={() => setActiveTab("mcp")}
                className={`w-full text-left px-3 py-2 rounded-md ${
                  activeTab === "mcp" ? "bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-100 font-medium" : "hover:bg-gray-200 dark:hover:bg-gray-700"
                }`}
              >
                MCP Servers
              </button>
            </li>
          </ul>
        </nav>
      </aside>

      {/* Main Content */}
      <main className="flex-1 flex flex-col overflow-hidden">
        <header className="h-14 border-b border-gray-200 dark:border-gray-700 flex items-center px-6">
          <h2 className="font-semibold text-lg capitalize">{activeTab.replace('-', ' ')}</h2>
        </header>
        <div className="flex-1 overflow-hidden p-6">
          {activeTab === "agents" && (
            <div className="h-full overflow-auto">
              <p className="text-gray-600 dark:text-gray-400 mb-4">Manage your locally running agents here.</p>
              <div className="p-6 bg-white dark:bg-gray-800 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700">
                <div className="flex justify-between items-center mb-4">
                  <h3 className="font-medium text-lg">Active Agents</h3>
                  <button className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-colors shadow-sm font-medium">
                    + Spawn New Agent
                  </button>
                </div>
                <div className="text-center py-10 text-gray-500 dark:text-gray-400 border-2 border-dashed border-gray-200 dark:border-gray-700 rounded-lg">
                  No agents are currently running.
                </div>
              </div>
            </div>
          )}
          {activeTab === "providers" && (
            <div className="h-full overflow-auto">
              <p className="text-gray-600 dark:text-gray-400 mb-4">Configure LLM API keys securely. Keys are stored in your OS keychain.</p>
              <Providers />
            </div>
          )}
          {activeTab === "skills" && (
            <div className="h-full">
              <Skills />
            </div>
          )}
          {activeTab === "mcp" && (
            <div className="h-full overflow-auto">
              <p className="text-gray-600 dark:text-gray-400 mb-4">Manage Model Context Protocol servers.</p>
              <div className="p-6 bg-white dark:bg-gray-800 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700">
                 <div className="flex justify-between items-center mb-4">
                    <h3 className="font-medium text-lg">Connected MCP Servers</h3>
                    <button className="px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-800 dark:text-gray-200 rounded-md hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors font-medium">
                      Add Server
                    </button>
                 </div>
                 <div className="text-center py-10 text-gray-500 dark:text-gray-400 border-2 border-dashed border-gray-200 dark:border-gray-700 rounded-lg">
                    No MCP servers configured.
                 </div>
              </div>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
