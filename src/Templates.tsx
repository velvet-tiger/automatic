import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, X, Edit2, FileText, Check, LayoutTemplate } from "lucide-react";
import { ICONS } from "./icons";

export default function Templates() {
  const [templates, setTemplates] = useState<string[]>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string | null>(null);
  const [templateContent, setTemplateContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [newTemplateName, setNewTemplateName] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadTemplates();
  }, []);

  const loadTemplates = async () => {
    try {
      const result: string[] = await invoke("get_templates");
      setTemplates(result.sort());
      setError(null);
    } catch (err: any) {
      setError(`Failed to load templates: ${err}`);
    }
  };

  const loadTemplateContent = async (name: string) => {
    try {
      const content: string = await invoke("read_template", { name });
      setSelectedTemplate(name);
      setTemplateContent(content);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
    } catch (err: any) {
      setError(`Failed to read template ${name}: ${err}`);
    }
  };

  const handleSave = async () => {
    if (!selectedTemplate && !isCreating) return;
    const name = isCreating ? newTemplateName.trim() : selectedTemplate!;
    if (!name) return;
    try {
      await invoke("save_template", { name, content: templateContent });
      setIsEditing(false);
      setSelectedTemplate(name);
      if (isCreating) {
        setIsCreating(false);
        await loadTemplates();
      }
      setError(null);
    } catch (err: any) {
      setError(`Failed to save template: ${err}`);
    }
  };

  const handleDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invoke("delete_template", { name });
      if (selectedTemplate === name) {
        setSelectedTemplate(null);
        setTemplateContent("");
        setIsEditing(false);
      }
      await loadTemplates();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete template: ${err}`);
    }
  };

  const startCreateNew = () => {
    setSelectedTemplate(null);
    setTemplateContent("");
    setIsCreating(true);
    setIsEditing(true);
    setNewTemplateName("");
  };

  return (
    <div className="flex h-full w-full bg-[#222327]">
      {/* Left Sidebar - Template List */}
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50">
        <div className="h-11 px-4 border-b border-[#33353A] flex justify-between items-center bg-[#222327]/30">
          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">Templates</span>
          <button
            onClick={startCreateNew}
            className="text-[#8A8C93] hover:text-[#E0E1E6] transition-colors p-1 hover:bg-[#2D2E36] rounded"
            title="Create New Template"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {templates.length === 0 && !isCreating ? (
            <div className="px-4 py-3 text-[13px] text-[#8A8C93] text-center">No templates yet.</div>
          ) : (
            <ul className="space-y-1 px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-[#2D2E36]">
                  <div className={ICONS.fileTemplate.iconBox}>
                    <LayoutTemplate size={15} className={ICONS.fileTemplate.iconColor} />
                  </div>
                  <span className="text-[13px] text-[#E0E1E6] italic">New Template...</span>
                </li>
              )}
              {templates.map(name => {
                const isActive = selectedTemplate === name && !isCreating;
                return (
                  <li key={name} className="group relative">
                    <button
                      onClick={() => loadTemplateContent(name)}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isActive
                          ? "bg-[#2D2E36] text-[#E0E1E6]"
                          : "text-[#8A8C93] hover:bg-[#2D2E36]/60 hover:text-[#E0E1E6]"
                      }`}
                    >
                      <div className={ICONS.fileTemplate.iconBox}>
                        <LayoutTemplate size={15} className={ICONS.fileTemplate.iconColor} />
                      </div>
                      <span className={`flex-1 text-[13px] font-medium truncate ${isActive ? "text-[#E0E1E6]" : "text-[#C8CAD0]"}`}>
                        {name}
                      </span>
                    </button>
                    <button
                      onClick={(e) => handleDelete(name, e)}
                      className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 hover:bg-[#33353A] rounded transition-all"
                      title="Delete Template"
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

        {(selectedTemplate || isCreating) ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="h-11 px-6 border-b border-[#33353A] flex justify-between items-center">
              <div className="flex items-center gap-3">
                <FileText size={14} className={ICONS.fileTemplate.iconColor} />
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="template-name (no spaces/slashes)"
                    value={newTemplateName}
                    onChange={(e) => setNewTemplateName(e.target.value)}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-[#E0E1E6] placeholder-[#8A8C93]/50 w-64"
                  />
                ) : (
                  <h3 className="text-[14px] font-medium text-[#E0E1E6]">{selectedTemplate}</h3>
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
                          loadTemplateContent(selectedTemplate!);
                        }}
                        className="px-3 py-1.5 hover:bg-[#2D2E36] text-[#8A8C93] hover:text-[#E0E1E6] rounded text-[12px] font-medium transition-colors"
                      >
                        Cancel
                      </button>
                    )}
                    <button
                      onClick={handleSave}
                      disabled={isCreating && !newTemplateName.trim()}
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
                  value={templateContent}
                  onChange={(e) => setTemplateContent(e.target.value)}
                  className="absolute inset-0 w-full h-full p-6 resize-none outline-none font-mono text-[13px] bg-[#222327] text-[#E0E1E6] leading-relaxed custom-scrollbar placeholder-[#8A8C93]/30"
                  placeholder="Write your project file template here in Markdown..."
                  spellCheck={false}
                />
              ) : (
                <div className="absolute inset-0 overflow-y-auto p-6 font-mono text-[13px] whitespace-pre-wrap text-[#E0E1E6] leading-relaxed custom-scrollbar">
                  {templateContent || <span className="text-[#8A8C93] italic">This template is empty. Click edit to add content.</span>}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-16 h-16 mx-auto mb-6 rounded-2xl bg-[#8B5CF6]/10 border border-[#8B5CF6]/20 flex items-center justify-center">
              <LayoutTemplate size={24} className={ICONS.fileTemplate.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-[#E0E1E6] mb-2">No Template Selected</h2>
            <p className="text-[14px] text-[#8A8C93] mb-8 leading-relaxed max-w-sm">
              Templates are reusable starting points for project files like CLAUDE.md or AGENTS.md. Create one to quickly initialize new projects.
            </p>
            <button
              onClick={startCreateNew}
              className="px-4 py-2 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              Create Template
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
