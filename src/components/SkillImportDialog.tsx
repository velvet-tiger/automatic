import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { X, Upload, Github, Loader2, CheckCircle2, FolderOpen, FileText } from "lucide-react";

interface ImportedSkill {
  name: string;
  source_path: string;
}

interface ImportedSkillFromRepo {
  name: string;
  source: string;
  id: string;
}

interface SkillImportDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onImport: (skillName: string) => void;
}

type ImportMode = "local" | "repository";
type ImportStatus = "idle" | "importing" | "success" | "error";

function parseInvokeResult<T>(value: unknown): T {
  if (typeof value === "string") {
    return JSON.parse(value) as T;
  }

  return value as T;
}

export default function SkillImportDialog({ isOpen, onClose, onImport }: SkillImportDialogProps) {
  const [mode, setMode] = useState<ImportMode>("local");
  const [repoUrl, setRepoUrl] = useState("");
  const [skillName, setSkillName] = useState("");
  const [status, setStatus] = useState<ImportStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [importedSkillNames, setImportedSkillNames] = useState<string[]>([]);

  const handleLocalImport = async () => {
    const selected = await open({
      multiple: false,
      filters: [
        { name: "Skill Files", extensions: ["skill", "md"] },
        { name: "All Files", extensions: ["*"] },
      ],
      title: "Select SKILL.md, .skill package, or skill directory",
    });

    if (!selected) return;

    setStatus("importing");
    setError(null);
    setImportedSkillNames([]);

    try {
      const path = typeof selected === "string" ? selected : (selected as unknown as string);
      
      // Check if it's a .skill package file
      const isPackage = path.toLowerCase().endsWith(".skill");
      
      const rawResult = isPackage
        ? await invoke("import_skill_from_package", { path })
        : await invoke("import_skill_from_local_path", { path });
      const result = parseInvokeResult<ImportedSkill[]>(rawResult);

      if (result.length === 0) {
        setError("No skills found at the selected path.");
        setStatus("error");
        return;
      }

      const importedNames = result.map((s) => s.name);
      setImportedSkillNames(importedNames);
      setStatus("success");
      onImport(importedNames[0]!);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  };

  const handleRepositoryImport = async () => {
    if (!repoUrl.trim()) {
      setError("Please enter a repository URL.");
      return;
    }

    setStatus("importing");
    setError(null);
    setImportedSkillNames([]);

    try {
      const rawResult = await invoke("import_skill_from_repository", {
        repoUrl: repoUrl.trim(),
        skillName: skillName.trim() || null,
      });
      const result = parseInvokeResult<ImportedSkillFromRepo[]>(rawResult);

      const importedNames = result.map((s) => s.name);
      setImportedSkillNames(importedNames);
      setStatus("success");
      onImport(importedNames[0]!);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  };

  const handleClose = () => {
    setStatus("idle");
    setError(null);
    setImportedSkillNames([]);
    setRepoUrl("");
    setSkillName("");
    onClose();
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={handleClose} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-lg mx-4">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40">
          <h2 className="text-[15px] font-semibold text-text-base">Import Skill</h2>
          <button
            onClick={handleClose}
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Mode Tabs */}
        <div className="px-5 pt-4">
          <div className="flex gap-1 p-1 bg-bg-sidebar rounded-lg">
            <button
              onClick={() => setMode("local")}
              className={`flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-md text-[13px] font-medium transition-colors ${
                mode === "local"
                  ? "bg-bg-base text-text-base"
                  : "text-text-muted hover:text-text-base"
              }`}
            >
              <FolderOpen size={14} />
              Local File
            </button>
            <button
              onClick={() => setMode("repository")}
              className={`flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-md text-[13px] font-medium transition-colors ${
                mode === "repository"
                  ? "bg-bg-base text-text-base"
                  : "text-text-muted hover:text-text-base"
              }`}
            >
              <Github size={14} />
              Repository URL
            </button>
          </div>
        </div>

        {/* Content */}
        <div className="px-5 py-4">
          {mode === "local" ? (
            <div className="space-y-4">
              <p className="text-[13px] text-text-muted leading-relaxed">
                Import a skill from your local filesystem. You can select:
              </p>
              <ul className="text-[13px] text-text-muted space-y-1.5 ml-4">
                <li className="flex items-start gap-2">
                  <span className="text-brand">•</span>
                  <span>A <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">.skill</code> package file (Claude skill format)</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-brand">•</span>
                  <span>A <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">SKILL.md</code> file to import directly</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-brand">•</span>
                  <span>A folder with <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">skill.json</code> at root</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-brand">•</span>
                  <span>A folder to scan for <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">SKILL.md</code> files (up to 3 levels deep)</span>
                </li>
              </ul>

              <button
                onClick={handleLocalImport}
                disabled={status === "importing"}
                className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {status === "importing" ? (
                  <>
                    <Loader2 size={16} className="animate-spin" />
                    Importing...
                  </>
                ) : (
                  <>
                    <Upload size={16} />
                    Select File or Folder
                  </>
                )}
              </button>
            </div>
          ) : (
            <div className="space-y-4">
              <p className="text-[13px] text-text-muted leading-relaxed">
                Import a skill from a GitHub repository. Enter the repository URL or owner/repo shorthand.
              </p>

              <div className="space-y-3">
                <div>
                  <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1.5">
                    Repository URL <span className="text-red-400">*</span>
                  </label>
                  <input
                    type="text"
                    value={repoUrl}
                    onChange={(e) => setRepoUrl(e.target.value)}
                    placeholder="https://github.com/owner/repo or owner/repo"
                    className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[13px] text-text-base placeholder-text-muted/40 font-mono"
                    spellCheck={false}
                  />
                </div>

                <div>
                  <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1.5">
                    Skill Name <span className="text-text-muted/60">(optional)</span>
                  </label>
                  <input
                    type="text"
                    value={skillName}
                    onChange={(e) => setSkillName(e.target.value)}
                    placeholder="Auto-detected from repository"
                    className="w-full px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[13px] text-text-base placeholder-text-muted/40"
                    spellCheck={false}
                  />
                </div>
              </div>

              <button
                onClick={handleRepositoryImport}
                disabled={status === "importing" || !repoUrl.trim()}
                className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {status === "importing" ? (
                  <>
                    <Loader2 size={16} className="animate-spin" />
                    Importing...
                  </>
                ) : (
                  <>
                    <Github size={16} />
                    Import from Repository
                  </>
                )}
              </button>
            </div>
          )}

          {/* Error */}
          {error && (
            <div className="mt-4 p-3 rounded-lg bg-red-500/10 border border-red-500/20">
              <p className="text-[12px] text-red-400">{error}</p>
            </div>
          )}

          {/* Success */}
          {status === "success" && importedSkillNames.length > 0 && (
            <div className="mt-4 p-3 rounded-lg bg-success/10 border border-success/20">
              <div className="flex items-center gap-2 mb-2">
                <CheckCircle2 size={14} className="text-success" />
                <span className="text-[12px] font-medium text-success">
                  Successfully imported {importedSkillNames.length} skill{importedSkillNames.length > 1 ? "s" : ""}
                </span>
              </div>
              <ul className="space-y-1">
                {importedSkillNames.map((name, idx) => (
                  <li key={idx} className="flex items-center gap-2 text-[12px] text-text-muted">
                    <FileText size={12} className="text-success" />
                    <span className="font-mono">{name}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-5 py-3 border-t border-border-strong/40">
          <button
            onClick={handleClose}
            className="px-4 py-2 text-[13px] font-medium text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded-lg transition-colors"
          >
            {status === "success" ? "Done" : "Cancel"}
          </button>
        </div>
      </div>
    </div>
  );
}
