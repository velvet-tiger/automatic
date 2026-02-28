import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, X, Edit2, FileText, Check, ScrollText } from "lucide-react";
import { ICONS } from "./icons";

interface RuleEntry {
  id: string;
  name: string;
}

interface Rule {
  name: string;
  content: string;
}

export default function Rules() {
  const [rules, setRules] = useState<RuleEntry[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [displayName, setDisplayName] = useState("");
  const [ruleContent, setRuleContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newMachineName, setNewMachineName] = useState("");
  const [newDisplayName, setNewDisplayName] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadRules();
  }, []);

  const loadRules = async () => {
    try {
      const result: RuleEntry[] = await invoke("get_rules");
      setRules(result.sort((a, b) => a.name.localeCompare(b.name)));
      setError(null);
    } catch (err: any) {
      setError(`Failed to load rules: ${err}`);
    }
  };

  const loadRule = async (id: string) => {
    try {
      const raw: string = await invoke("read_rule", { machineName: id });
      const rule: Rule = JSON.parse(raw);
      setSelectedId(id);
      setDisplayName(rule.name);
      setRuleContent(rule.content);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
    } catch (err: any) {
      setError(`Failed to read rule: ${err}`);
    }
  };

  const handleSave = async () => {
    if (isCreating) {
      const id = newMachineName.trim();
      const name = newDisplayName.trim();
      if (!id || !name) return;
      try {
        await invoke("save_rule", { machineName: id, name, content: ruleContent });
        setIsEditing(false);
        setIsCreating(false);
        await loadRules();
        setSelectedId(id);
        setDisplayName(name);
        setError(null);
      } catch (err: any) {
        setError(`Failed to save rule: ${err}`);
      }
    } else if (selectedId) {
      try {
        await invoke("save_rule", { machineName: selectedId, name: displayName, content: ruleContent });
        setIsEditing(false);
        // Update sidebar entry in-place
        setRules(prev => prev.map(r => r.id === selectedId ? { ...r, name: displayName } : r));
        setError(null);
      } catch (err: any) {
        setError(`Failed to save rule: ${err}`);
      }
    }
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete rule "${id}"?`, { title: "Delete Rule", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_rule", { machineName: id });
      if (selectedId === id) {
        setSelectedId(null);
        setDisplayName("");
        setRuleContent("");
        setIsEditing(false);
      }
      await loadRules();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete rule: ${err}`);
    }
  };

  const startCreateNew = () => {
    setSelectedId(null);
    setDisplayName("");
    setRuleContent("");
    setIsCreating(true);
    setIsEditing(true);
    setNewMachineName("");
    setNewDisplayName("");
  };

  const selectedEntry = rules.find(r => r.id === selectedId);

  return (
    <div className="flex h-full w-full bg-[#222327]">
      {/* Left Sidebar - Rule List */}
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50">
        <div className="h-11 px-4 border-b border-[#33353A] flex justify-between items-center bg-[#222327]/30">
          <span className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">Rules</span>
          <button
            onClick={startCreateNew}
            className="text-[#C8CAD0] hover:text-[#F8F8FA] transition-colors p-1 hover:bg-[#2D2E36] rounded"
            title="Create New Rule"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {rules.length === 0 && !isCreating ? (
            <div className="px-4 py-3 text-[13px] text-[#C8CAD0] text-center">No rules yet.</div>
          ) : (
            <ul className="space-y-1 px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-[#2D2E36]">
                  <div className={ICONS.rule.iconBox}>
                    <ScrollText size={15} className={ICONS.rule.iconColor} />
                  </div>
                  <span className="text-[13px] text-[#F8F8FA] italic">New Rule...</span>
                </li>
              )}
              {rules.map(entry => {
                const isActive = selectedId === entry.id && !isCreating;
                return (
                  <li key={entry.id} className="group relative">
                    <button
                      onClick={() => loadRule(entry.id)}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isActive
                          ? "bg-[#2D2E36] text-[#F8F8FA]"
                          : "text-[#C8CAD0] hover:bg-[#2D2E36]/60 hover:text-[#F8F8FA]"
                      }`}
                    >
                      <div className={ICONS.rule.iconBox}>
                        <ScrollText size={15} className={ICONS.rule.iconColor} />
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className={`text-[13px] font-medium truncate ${isActive ? "text-[#F8F8FA]" : "text-[#E8E9ED]"}`}>
                          {entry.name}
                        </div>
                        <div className="text-[10px] text-[#C8CAD0] truncate">{entry.id}</div>
                      </div>
                    </button>
                    <button
                      onClick={(e) => handleDelete(entry.id, e)}
                      className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-[#C8CAD0] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 hover:bg-[#33353A] rounded transition-all"
                      title="Delete Rule"
                    >
                      <X size={12} />
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      {/* Right Area - Editor/Viewer */}
      <div className="flex-1 flex flex-col min-w-0 bg-[#222327]">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between">
            {error}
            <button onClick={() => setError(null)}><X size={14} /></button>
          </div>
        )}

        {(selectedId || isCreating) ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="min-h-[44px] px-6 border-b border-[#33353A] flex justify-between items-center gap-4 py-2">
              <div className="flex items-center gap-3 min-w-0 flex-1">
                <FileText size={14} className={ICONS.rule.iconColor + " flex-shrink-0"} />
                {isCreating ? (
                  <div className="flex flex-col gap-1.5 min-w-0">
                    <input
                      type="text"
                      placeholder="Display Name"
                      value={newDisplayName}
                      onChange={(e) => setNewDisplayName(e.target.value)}
                      autoFocus
                      className="bg-transparent border-none outline-none text-[14px] font-medium text-[#F8F8FA] placeholder-[#C8CAD0]/50 w-72"
                    />
                    <div className="flex items-center gap-2">
                      <input
                        type="text"
                        placeholder="machine-name (lowercase, hyphens)"
                        value={newMachineName}
                        onChange={(e) => setNewMachineName(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ''))}
                        className="bg-transparent border-none outline-none text-[11px] text-[#C8CAD0] placeholder-[#C8CAD0]/40 font-mono w-72"
                      />
                    </div>
                  </div>
                ) : isEditing ? (
                  <div className="flex flex-col gap-0.5 min-w-0">
                    <input
                      type="text"
                      value={displayName}
                      onChange={(e) => setDisplayName(e.target.value)}
                      className="bg-transparent border-none outline-none text-[14px] font-medium text-[#F8F8FA] placeholder-[#C8CAD0]/50 w-72"
                      placeholder="Display Name"
                    />
                    <span className="text-[10px] text-[#C8CAD0] font-mono">{selectedId}</span>
                  </div>
                ) : (
                  <div className="flex flex-col gap-0.5 min-w-0">
                    <h3 className="text-[14px] font-medium text-[#F8F8FA] truncate">{selectedEntry?.name || displayName}</h3>
                    <span className="text-[10px] text-[#C8CAD0] font-mono">{selectedId}</span>
                  </div>
                )}
              </div>

              <div className="flex items-center gap-2 flex-shrink-0">
                {!isEditing ? (
                  <button
                    onClick={() => setIsEditing(true)}
                    className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-[#2D2E36] text-[#C8CAD0] hover:text-[#F8F8FA] rounded text-[12px] font-medium transition-colors"
                  >
                    <Edit2 size={12} /> Edit
                  </button>
                ) : (
                  <>
                    {!isCreating && (
                      <button
                        onClick={() => {
                          setIsEditing(false);
                          if (selectedId) loadRule(selectedId);
                        }}
                        className="px-3 py-1.5 hover:bg-[#2D2E36] text-[#C8CAD0] hover:text-[#F8F8FA] rounded text-[12px] font-medium transition-colors"
                      >
                        Cancel
                      </button>
                    )}
                    <button
                      onClick={handleSave}
                      disabled={isCreating ? (!newMachineName.trim() || !newDisplayName.trim()) : false}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                    >
                      <Check size={12} /> Save
                    </button>
                  </>
                )}
              </div>
            </div>

            {/* Editor Body */}
            <div className="flex-1 relative">
              {isEditing ? (
                <textarea
                  value={ruleContent}
                  onChange={(e) => setRuleContent(e.target.value)}
                  className="absolute inset-0 w-full h-full p-6 resize-none outline-none font-mono text-[13px] bg-[#222327] text-[#F8F8FA] leading-relaxed custom-scrollbar placeholder-[#C8CAD0]/30"
                  placeholder="Write your rule content here in Markdown. Rules are reusable content blocks that can be appended to project instruction files..."
                  spellCheck={false}
                />
              ) : (
                <div className="absolute inset-0 overflow-y-auto p-6 font-mono text-[13px] whitespace-pre-wrap text-[#F8F8FA] leading-relaxed custom-scrollbar">
                  {ruleContent || <span className="text-[#C8CAD0] italic">This rule is empty. Click edit to add content.</span>}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-16 h-16 mx-auto mb-6 rounded-2xl bg-[#22D3EE]/10 border border-[#22D3EE]/20 flex items-center justify-center">
              <ScrollText size={24} className={ICONS.rule.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-[#F8F8FA] mb-2">No Rule Selected</h2>
            <p className="text-[14px] text-[#C8CAD0] mb-8 leading-relaxed max-w-sm">
              Rules are reusable content blocks that can be appended to project instruction files. Add rules to share common guidelines across projects.
            </p>
            <button
              onClick={startCreateNew}
              className="px-4 py-2 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              Create Rule
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
