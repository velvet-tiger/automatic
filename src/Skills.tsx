import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, X, Edit2, Code, FileText, Check } from "lucide-react";

export default function Skills() {
  const [skills, setSkills] = useState<string[]>([]);
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [newSkillName, setNewSkillName] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadSkills();
  }, []);

  const loadSkills = async () => {
    try {
      const result: string[] = await invoke("get_skills");
      setSkills(result.sort());
      setError(null);
    } catch (err: any) {
      setError(`Failed to load skills: ${err}`);
    }
  };

  const loadSkillContent = async (name: string) => {
    try {
      const content: string = await invoke("read_skill", { name });
      setSelectedSkill(name);
      setSkillContent(content);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
    } catch (err: any) {
      setError(`Failed to read skill ${name}: ${err}`);
    }
  };

  const handleSave = async () => {
    if (!selectedSkill) return;
    try {
      await invoke("save_skill", { name: selectedSkill, content: skillContent });
      setIsEditing(false);
      if (isCreating) {
        setIsCreating(false);
        await loadSkills();
      }
      setError(null);
    } catch (err: any) {
      setError(`Failed to save skill: ${err}`);
    }
  };

  const handleDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirm(`Are you sure you want to delete the skill "${name}"?`)) return;
    try {
      await invoke("delete_skill", { name });
      if (selectedSkill === name) {
        setSelectedSkill(null);
        setSkillContent("");
        setIsEditing(false);
      }
      await loadSkills();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete skill: ${err}`);
    }
  };

  const startCreateNew = () => {
    setSelectedSkill(null);
    setSkillContent("");
    setIsCreating(true);
    setIsEditing(true);
    setNewSkillName("");
  };

  return (
    <div className="flex h-full w-full bg-[#222327]">
      {/* Left Sidebar - Skills List */}
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50">
        <div className="h-11 px-4 border-b border-[#33353A] flex justify-between items-center bg-[#222327]/30">
          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">Your Skills</span>
          <button 
            onClick={startCreateNew}
            className="text-[#8A8C93] hover:text-[#E0E1E6] transition-colors p-1 hover:bg-[#2D2E36] rounded"
            title="Create New Skill"
          >
            <Plus size={14} />
          </button>
        </div>
        
        <div className="flex-1 overflow-y-auto py-2">
          {skills.length === 0 && !isCreating ? (
            <div className="px-4 py-3 text-[13px] text-[#8A8C93] text-center">No skills found.</div>
          ) : (
            <ul className="space-y-0.5 px-2">
              {isCreating && (
                <li className="flex items-center gap-2.5 px-2 py-1.5 rounded-md text-[13px] bg-[#2D2E36] text-[#E0E1E6]">
                  <Code size={14} className="text-[#8A8C93]" />
                  <span className="italic">New Skill...</span>
                </li>
              )}
              {skills.map(skill => (
                <li key={skill} className="group flex items-center relative">
                  <button
                    onClick={() => loadSkillContent(skill)}
                    className={`w-full flex items-center gap-2.5 px-2 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
                      selectedSkill === skill && !isCreating
                        ? "bg-[#2D2E36] text-[#E0E1E6]" 
                        : "text-[#8A8C93] hover:bg-[#2D2E36]/50 hover:text-[#E0E1E6]"
                    }`}
                  >
                    <Code size={14} className={selectedSkill === skill && !isCreating ? "text-[#E0E1E6]" : "text-[#8A8C93]"} />
                    <span className="flex-1 text-left truncate">{skill}</span>
                  </button>
                  <button
                    onClick={(e) => handleDelete(skill, e)}
                    className="absolute right-2 p-1 text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 hover:bg-[#33353A] rounded transition-all"
                    title="Delete Skill"
                  >
                    <X size={12} />
                  </button>
                </li>
              ))}
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

        {(selectedSkill || isCreating) ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="h-11 px-6 border-b border-[#33353A] flex justify-between items-center">
              <div className="flex items-center gap-3">
                <FileText size={14} className="text-[#8A8C93]" />
                {isCreating ? (
                  <input 
                    type="text" 
                    placeholder="Skill-name (no spaces/slashes)"
                    value={newSkillName}
                    onChange={(e) => {
                      setNewSkillName(e.target.value);
                      setSelectedSkill(e.target.value);
                    }}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-[#E0E1E6] placeholder-[#8A8C93]/50 w-64"
                  />
                ) : (
                  <h3 className="text-[14px] font-medium text-[#E0E1E6]">{selectedSkill}</h3>
                )}
              </div>
              
              <div className="flex items-center gap-2">
                {!isEditing ? (
                  <button 
                    onClick={() => setIsEditing(true)}
                    className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-[#2D2E36] text-[#8A8C93] hover:text-[#E0E1E6] rounded text-[12px] font-medium transition-colors"
                  >
                    <Edit2 size={12} /> Edit
                  </button>
                ) : (
                  <>
                    {!isCreating && (
                      <button 
                        onClick={() => {
                          setIsEditing(false);
                          loadSkillContent(selectedSkill!);
                        }}
                        className="px-3 py-1.5 hover:bg-[#2D2E36] text-[#8A8C93] hover:text-[#E0E1E6] rounded text-[12px] font-medium transition-colors"
                      >
                        Cancel
                      </button>
                    )}
                    <button 
                      onClick={handleSave}
                      disabled={isCreating && !newSkillName.trim()}
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
                  value={skillContent}
                  onChange={(e) => setSkillContent(e.target.value)}
                  className="absolute inset-0 w-full h-full p-6 resize-none outline-none font-mono text-[13px] bg-[#222327] text-[#E0E1E6] leading-relaxed custom-scrollbar placeholder-[#8A8C93]/30"
                  placeholder="Write your skill instructions here in Markdown..."
                  spellCheck={false}
                />
              ) : (
                <div className="absolute inset-0 overflow-y-auto p-6 font-mono text-[13px] whitespace-pre-wrap text-[#E0E1E6] leading-relaxed custom-scrollbar">
                  {skillContent || <span className="text-[#8A8C93] italic">This skill is empty. Click edit to add instructions.</span>}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
             <div className="w-16 h-16 mx-auto mb-6 rounded-full border border-dashed border-[#44474F] flex items-center justify-center text-[#8A8C93]">
                <Code size={24} strokeWidth={1.5} />
              </div>
              <h2 className="text-lg font-medium text-[#E0E1E6] mb-2">No Skill Selected</h2>
              <p className="text-[14px] text-[#8A8C93] mb-8 leading-relaxed max-w-sm">
                Select a skill from the sidebar to view or edit its contents, or create a new one to extend agent capabilities.
              </p>
          </div>
        )}
      </div>
    </div>
  );
}