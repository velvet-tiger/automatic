import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

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
      setSkills(result);
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

  const handleDelete = async (name: string) => {
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
    <div className="flex h-full gap-6">
      {/* Left Sidebar - Skills List */}
      <div className="w-64 flex flex-col bg-white dark:bg-gray-800 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700 overflow-hidden">
        <div className="p-4 border-b border-gray-200 dark:border-gray-700 flex justify-between items-center">
          <h3 className="font-semibold text-gray-800 dark:text-gray-200">Your Skills</h3>
          <button 
            onClick={startCreateNew}
            className="text-xl text-blue-600 dark:text-blue-400 hover:text-blue-800"
            title="Create New Skill"
          >
            +
          </button>
        </div>
        <div className="flex-1 overflow-y-auto">
          {skills.length === 0 ? (
            <div className="p-4 text-sm text-gray-500 italic">No skills found.</div>
          ) : (
            <ul className="divide-y divide-gray-100 dark:divide-gray-700">
              {skills.map(skill => (
                <li key={skill} className="flex group">
                  <button
                    onClick={() => loadSkillContent(skill)}
                    className={`flex-1 text-left px-4 py-3 text-sm transition-colors ${
                      selectedSkill === skill 
                        ? "bg-blue-50 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 font-medium" 
                        : "hover:bg-gray-50 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300"
                    }`}
                  >
                    {skill}
                  </button>
                  <button
                    onClick={() => handleDelete(skill)}
                    className="px-3 text-red-500 opacity-0 group-hover:opacity-100 hover:bg-red-50 dark:hover:bg-red-900/20 transition-all"
                    title="Delete Skill"
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      {/* Right Area - Editor/Viewer */}
      <div className="flex-1 flex flex-col bg-white dark:bg-gray-800 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700 overflow-hidden">
        {error && (
          <div className="bg-red-100 text-red-700 p-3 text-sm border-b border-red-200">
            {error}
          </div>
        )}

        {(selectedSkill || isCreating) ? (
          <div className="flex-1 flex flex-col h-full">
            <div className="p-4 border-b border-gray-200 dark:border-gray-700 flex justify-between items-center bg-gray-50 dark:bg-gray-800/50">
              {isCreating ? (
                <input 
                  type="text" 
                  placeholder="skill-name (no spaces/slashes)"
                  value={newSkillName}
                  onChange={(e) => {
                    setNewSkillName(e.target.value);
                    setSelectedSkill(e.target.value);
                  }}
                  className="font-semibold text-lg bg-white dark:bg-gray-700 border border-gray-300 dark:border-gray-600 rounded px-2 py-1 outline-none"
                />
              ) : (
                <h3 className="font-semibold text-lg text-gray-800 dark:text-gray-200">{selectedSkill}</h3>
              )}
              
              <div className="space-x-2">
                {!isEditing ? (
                  <button 
                    onClick={() => setIsEditing(true)}
                    className="px-3 py-1.5 bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-800 dark:text-gray-200 rounded transition-colors text-sm font-medium"
                  >
                    Edit
                  </button>
                ) : (
                  <>
                    {!isCreating && (
                      <button 
                        onClick={() => {
                          setIsEditing(false);
                          loadSkillContent(selectedSkill!);
                        }}
                        className="px-3 py-1.5 text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors text-sm"
                      >
                        Cancel
                      </button>
                    )}
                    <button 
                      onClick={handleSave}
                      disabled={isCreating && !newSkillName.trim()}
                      className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded transition-colors text-sm font-medium disabled:opacity-50"
                    >
                      Save
                    </button>
                  </>
                )}
              </div>
            </div>
            
            <div className="flex-1 p-0 relative">
              {isEditing ? (
                <textarea 
                  value={skillContent}
                  onChange={(e) => setSkillContent(e.target.value)}
                  className="absolute inset-0 w-full h-full p-4 resize-none outline-none font-mono text-sm bg-gray-50 dark:bg-gray-900 text-gray-800 dark:text-gray-200"
                  placeholder="Write your skill instructions here..."
                />
              ) : (
                <div className="absolute inset-0 overflow-y-auto p-6 font-mono text-sm whitespace-pre-wrap text-gray-800 dark:text-gray-300">
                  {skillContent || <span className="text-gray-400 italic">This skill is empty. Click edit to add instructions.</span>}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="flex-1 flex items-center justify-center text-gray-400 dark:text-gray-500 italic">
            Select a skill from the list or create a new one.
          </div>
        )}
      </div>
    </div>
  );
}