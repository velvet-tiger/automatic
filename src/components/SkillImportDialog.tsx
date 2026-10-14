import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { X, Download, Github, Loader2, CheckCircle2, FolderOpen, FileText, Package, Server, AlertTriangle } from "lucide-react";

interface ImportedSkill {
  name: string;
  source_path: string;
  was_updated?: boolean;
}

interface ImportedSkillFromRepo {
  name: string;
  source: string;
  id: string;
  was_updated?: boolean;
}

interface ImportedSkillSummary {
  name: string;
  wasUpdated: boolean;
}

type AgentPackageItemKind = "skill" | "mcp_server";

interface AgentPackageSkippedItem {
  kind: AgentPackageItemKind;
  name: string;
  reason: string;
}

interface AgentPackageImportResult {
  package_name: string;
  package_root: string;
  data_dir: string;
  imported_skills: string[];
  imported_mcp_servers: string[];
  conflicts: AgentPackageSkippedItem[];
  invalid: AgentPackageSkippedItem[];
  warnings: string[];
}

interface AgentPackageSummary {
  path: string;
  name: string;
  version: string | null;
  description: string | null;
  skill_count: number;
  mcp_server_count: number;
}

interface AgentPackageListError {
  path: string;
  error: string;
}

interface InstalledAgentPackages {
  packages: AgentPackageSummary[];
  errors: AgentPackageListError[];
}

interface AgentPackageMcpConfig {
  type: "stdio" | "http" | "sse";
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  url?: string;
  headers?: Record<string, string>;
}

interface AgentPackageMcpServer {
  name: string;
  config: AgentPackageMcpConfig;
}

interface AgentPackageSkill {
  name: string;
  dir: string;
}

interface AgentPackage {
  root: string;
  name: string;
  version: string | null;
  description: string | null;
  data_dir: string;
  skills: AgentPackageSkill[];
  mcp_servers: AgentPackageMcpServer[];
  invalid: AgentPackageSkippedItem[];
  warnings: string[];
  digest: string;
}

interface AgentPackagePlannedItem {
  name: string;
  projects: string[];
}

interface AgentPackagePlan {
  skills: AgentPackagePlannedItem[];
  mcp_servers: AgentPackagePlannedItem[];
  conflicts: AgentPackageSkippedItem[];
}

interface AgentPackagePreview {
  package: AgentPackage;
  plan: AgentPackagePlan;
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
  const [importedSkills, setImportedSkills] = useState<ImportedSkillSummary[]>([]);
  const [packageResult, setPackageResult] = useState<AgentPackageImportResult | null>(null);
  const [packagePreview, setPackagePreview] = useState<AgentPackagePreview | null>(null);
  const [installedPackages, setInstalledPackages] = useState<InstalledAgentPackages | null>(null);

  const loadInstalledPackages = useCallback(async () => {
    try {
      setInstalledPackages(await invoke<InstalledAgentPackages>("list_installed_agent_packages"));
    } catch (e: unknown) {
      setInstalledPackages({ packages: [], errors: [{ path: "~/.agents/plugins", error: String(e) }] });
    }
  }, []);

  useEffect(() => {
    if (isOpen) void loadInstalledPackages();
  }, [isOpen, loadInstalledPackages]);

  const resetResults = () => {
    setStatus("importing");
    setError(null);
    setImportedSkills([]);
    setPackageResult(null);
    setPackagePreview(null);
  };

  // Packages can carry commands that agents will run, so the user sees
  // exactly what will be added before anything is written.
  const previewAgentPackage = async (path: string) => {
    resetResults();
    try {
      setPackagePreview(await invoke<AgentPackagePreview>("preview_agent_package", { path }));
      setStatus("idle");
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  };

  const importAgentPackage = async (path: string, expectedDigest: string) => {
    // The preview stays on screen while the import runs, so its button can show progress.
    setStatus("importing");
    setError(null);
    try {
      const result = await invoke<AgentPackageImportResult>("import_agent_package", { path, expectedDigest });
      setPackagePreview(null);
      setPackageResult(result);
      setStatus("success");
      const firstSkill = result.imported_skills[0];
      if (firstSkill) onImport(firstSkill);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  };

  const handleFolderImport = async () => {
    const selected = await open({
      multiple: false,
      directory: true,
      title: "Select a skill folder or an Agent Plugin package folder",
    });
    if (typeof selected !== "string") return;
    await importLocalPath(selected);
  };

  const handleLocalImport = async () => {
    const selected = await open({
      multiple: false,
      filters: [
        { name: "Skill Files", extensions: ["skill", "md", "json"] },
        { name: "All Files", extensions: ["*"] },
      ],
      title: "Select SKILL.md, .skill package, or plugin.json",
    });

    if (typeof selected !== "string") return;
    await importLocalPath(selected);
  };

  const importLocalPath = async (path: string) => {
    // A chosen plugin.json stands for the package folder that holds it.
    const pluginJsonSuffix = /[\\/]plugin\.json$/;
    if (pluginJsonSuffix.test(path)) {
      await previewAgentPackage(path.replace(pluginJsonSuffix, ""));
      return;
    }
    const isSkillPackage = path.toLowerCase().endsWith(".skill");
    // A failed check falls through to the plain skill import, which reports its own error.
    const isAgentPackage = !isSkillPackage
      && (await invoke<boolean>("is_agent_package_dir", { path }).catch(() => false));
    if (isAgentPackage) {
      await previewAgentPackage(path);
      return;
    }

    resetResults();

    try {
      const isPackage = isSkillPackage;

      const rawResult = isPackage
        ? await invoke("import_skill_from_package", { path })
        : await invoke("import_skill_from_local_path", { path });
      const result = parseInvokeResult<ImportedSkill[]>(rawResult);

      if (result.length === 0) {
        setError("No skills found at the selected path.");
        setStatus("error");
        return;
      }

      const summary = result.map((s) => ({ name: s.name, wasUpdated: s.was_updated ?? false }));
      setImportedSkills(summary);
      setStatus("success");
      onImport(summary[0]!.name);
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

    resetResults();

    try {
      const rawResult = await invoke("import_skill_from_repository", {
        repoUrl: repoUrl.trim(),
        skillName: skillName.trim() || null,
      });
      const result = parseInvokeResult<ImportedSkillFromRepo[]>(rawResult);

      if (result.length === 0) {
        setError("No skills found in the repository.");
        setStatus("error");
        return;
      }

      const summary = result.map((s) => ({ name: s.name, wasUpdated: s.was_updated ?? false }));
      setImportedSkills(summary);
      setStatus("success");
      onImport(summary[0]!.name);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  };

  const handleClose = () => {
    setStatus("idle");
    setError(null);
    setImportedSkills([]);
    setPackageResult(null);
    setPackagePreview(null);
    setRepoUrl("");
    setSkillName("");
    onClose();
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={handleClose} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto">
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
                <li className="flex items-start gap-2">
                  <span className="text-brand">•</span>
                  <span>An Agent Plugin package folder, with <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">plugin.json</code> at root. Its skills and MCP servers are added.</span>
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
                    <Download size={16} />
                    Select File
                  </>
                )}
              </button>
              <button
                onClick={handleFolderImport}
                disabled={status === "importing"}
                className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-bg-sidebar hover:bg-bg-base border border-border-strong/40 text-text-base rounded-lg text-[13px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <FolderOpen size={16} />
                Select Folder
              </button>

              {installedPackages && (installedPackages.packages.length > 0 || installedPackages.errors.length > 0) && (
                <div>
                  <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1.5">
                    Agent Plugin packages in ~/.agents/plugins
                  </div>
                  <ul className="bg-bg-sidebar border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/40">
                    {installedPackages.packages.map((pkg) => (
                      <li key={pkg.path} className="flex items-center gap-2 px-3 py-2 text-[12px]">
                        <Package size={12} className="text-text-muted shrink-0" />
                        <div className="min-w-0 flex-1">
                          <div className="font-mono text-text-base truncate">
                            {pkg.name}
                            {pkg.version ? <span className="text-text-muted"> {pkg.version}</span> : null}
                          </div>
                          <div className="text-text-muted truncate">
                            {pkg.skill_count} skill{pkg.skill_count === 1 ? "" : "s"}, {pkg.mcp_server_count} MCP server{pkg.mcp_server_count === 1 ? "" : "s"}
                          </div>
                        </div>
                        <button
                          onClick={() => void previewAgentPackage(pkg.path)}
                          disabled={status === "importing"}
                          className="px-2.5 py-1 rounded-md text-[12px] font-medium text-brand hover:bg-brand/10 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          Import
                        </button>
                      </li>
                    ))}
                    {installedPackages.errors.map((item) => (
                      <li key={item.path} className="px-3 py-2 text-[12px] text-text-muted">
                        <span className="font-mono">{item.path.split(/[\\/]/).pop()}</span>: {item.error}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
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
          {status === "success" && importedSkills.length > 0 && (() => {
            const total = importedSkills.length;
            const updatedCount = importedSkills.filter((s) => s.wasUpdated).length;
            const addedCount = total - updatedCount;
            const breakdownParts: string[] = [];
            if (addedCount > 0) breakdownParts.push(`${addedCount} added`);
            if (updatedCount > 0) breakdownParts.push(`${updatedCount} updated`);
            const breakdown = breakdownParts.join(" · ");
            return (
              <div className="mt-4 p-3 rounded-lg bg-success/10 border border-success/20">
                <div className="flex items-center gap-2 mb-2">
                  <CheckCircle2 size={14} className="text-success" />
                  <span className="text-[12px] font-medium text-success">
                    Imported {total} skill{total > 1 ? "s" : ""}
                    {breakdown ? ` (${breakdown})` : ""}
                  </span>
                </div>
                <ul className="space-y-1">
                  {importedSkills.map((skill, idx) => (
                    <li key={idx} className="flex items-center gap-2 text-[12px] text-text-muted">
                      <FileText size={12} className="text-success" />
                      <span className="font-mono">{skill.name}</span>
                      <span
                        className={`ml-auto px-1.5 py-0.5 rounded text-[10px] font-medium uppercase tracking-wider ${
                          skill.wasUpdated
                            ? "bg-brand/15 text-brand"
                            : "bg-success/15 text-success"
                        }`}
                      >
                        {skill.wasUpdated ? "Updated" : "New"}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            );
          })()}

          {packagePreview && status !== "success" && (
            <AgentPackagePreviewPanel
              preview={packagePreview}
              busy={status === "importing"}
              onConfirm={() => void importAgentPackage(packagePreview.package.root, packagePreview.package.digest)}
              onCancel={() => setPackagePreview(null)}
            />
          )}

          {status === "success" && packageResult && (
            <AgentPackageResultPanel result={packageResult} />
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

function AgentPackageResultPanel({ result }: { result: AgentPackageImportResult }) {
  const skipped = [...result.conflicts, ...result.invalid];
  const importedCount = result.imported_skills.length + result.imported_mcp_servers.length;
  return (
    <div className="mt-4 space-y-3">
      {importedCount === 0 ? (
        <div className="p-3 rounded-lg bg-bg-sidebar border border-border-strong/40">
          <span className="text-[12px] font-medium text-text-base">
            Nothing was imported from {result.package_name}.
          </span>
        </div>
      ) : (
      <div className="p-3 rounded-lg bg-success/10 border border-success/20">
        <div className="flex items-center gap-2 mb-2">
          <CheckCircle2 size={14} className="text-success" />
          <span className="text-[12px] font-medium text-success">
            Imported {importedCount} item{importedCount === 1 ? "" : "s"} from {result.package_name}
          </span>
        </div>
        <ul className="space-y-1">
          {result.imported_skills.map((name) => (
            <li key={`skill-${name}`} className="flex items-center gap-2 text-[12px] text-text-muted">
              <FileText size={12} className="text-success" />
              <span className="font-mono">{name}</span>
              <span className="ml-auto text-[10px] uppercase tracking-wider">Skill</span>
            </li>
          ))}
          {result.imported_mcp_servers.map((name) => (
            <li key={`mcp-${name}`} className="flex items-center gap-2 text-[12px] text-text-muted">
              <Server size={12} className="text-success" />
              <span className="font-mono">{name}</span>
              <span className="ml-auto text-[10px] uppercase tracking-wider">MCP server</span>
            </li>
          ))}
        </ul>
        {result.imported_mcp_servers.length > 0 && (
          <p className="mt-2 text-[12px] text-text-muted leading-relaxed">
            Local MCP servers run from the package folder. Keep it at{" "}
            <span className="font-mono break-all">{result.package_root}</span>.
          </p>
        )}
      </div>
      )}

      {(skipped.length > 0 || result.warnings.length > 0) && (
        <div className="p-3 rounded-lg bg-warning/10 border border-warning/20">
          <div className="flex items-center gap-2 mb-2">
            <AlertTriangle size={14} className="text-warning" />
            <span className="text-[12px] font-medium text-warning">Not imported</span>
          </div>
          <ul className="space-y-1 text-[12px] text-text-muted">
            {skipped.map((item) => (
              <li key={`${item.kind}-${item.name}`}>
                <span className="font-mono">{item.name}</span>
                {" "}({item.kind === "skill" ? "skill" : "MCP server"}): {item.reason}
              </li>
            ))}
            {result.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function describeServer(config: AgentPackageMcpConfig): string {
  if (config.type === "stdio") {
    return [config.command ?? "", ...(config.args ?? [])].join(" ");
  }
  return config.url ?? "";
}

function AgentPackagePreviewPanel({
  preview,
  busy,
  onConfirm,
  onCancel,
}: {
  preview: AgentPackagePreview;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const pkg = preview.package;
  const plan = preview.plan;
  const plannedServers = plan.mcp_servers.flatMap((item) => {
    const server = pkg.mcp_servers.find((s) => s.name === item.name);
    return server ? [{ server, projects: item.projects }] : [];
  });
  const skipped = [...plan.conflicts, ...pkg.invalid];
  const nothingToImport = plan.skills.length === 0 && plan.mcp_servers.length === 0;
  return (
    <div className="mt-4 space-y-3">
      <div className="p-3 rounded-lg bg-bg-sidebar border border-border-strong/40 space-y-3">
        <div>
          <div className="text-[13px] font-semibold text-text-base">
            {pkg.name}
            {pkg.version ? <span className="font-normal text-text-muted"> {pkg.version}</span> : null}
          </div>
          {pkg.description && <p className="text-[12px] text-text-muted">{pkg.description}</p>}
          <p className="text-[12px] text-text-muted font-mono break-all">{pkg.root}</p>
        </div>

        {plan.skills.length > 0 && (
          <div>
            <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1">Skills</div>
            <ul className="space-y-1 text-[12px] text-text-muted">
              {plan.skills.map((skill) => (
                <li key={skill.name}>
                  <span className="font-mono text-text-base">{skill.name}</span>
                  {skill.projects.length > 0 && (
                    <span className="text-warning"> · will be added to projects: {skill.projects.join(", ")}</span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        )}

        {plannedServers.length > 0 && (
          <div>
            <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1">MCP servers</div>
            <ul className="space-y-2 text-[12px] text-text-muted">
              {plannedServers.map(({ server, projects }) => {
                const envKeys = Object.keys(server.config.env ?? {});
                const headerKeys = Object.keys(server.config.headers ?? {});
                return (
                  <li key={server.name} className="space-y-0.5">
                    <div>
                      <span className="font-mono text-text-base">{server.name}</span>
                      <span> ({server.config.type})</span>
                    </div>
                    <div className="font-mono break-all">{describeServer(server.config)}</div>
                    {server.config.cwd && <div>Runs in: <span className="font-mono break-all">{server.config.cwd}</span></div>}
                    {envKeys.length > 0 && <div>Environment: <span className="font-mono">{envKeys.join(", ")}</span></div>}
                    {headerKeys.length > 0 && <div>Headers: <span className="font-mono">{headerKeys.join(", ")}</span></div>}
                    {projects.length > 0 && <div className="text-warning">Will be added to projects: {projects.join(", ")}</div>}
                  </li>
                );
              })}
            </ul>
          </div>
        )}

        {(skipped.length > 0 || pkg.warnings.length > 0) && (
          <div>
            <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-1">Will not be imported</div>
            <ul className="space-y-1 text-[12px] text-text-muted">
              {skipped.map((item) => (
                <li key={`${item.kind}-${item.name}`}>
                  <span className="font-mono">{item.name}</span>: {item.reason}
                </li>
              ))}
              {pkg.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          </div>
        )}
      </div>

      <div className="flex justify-end gap-2">
        <button
          onClick={onCancel}
          disabled={busy}
          className="px-4 py-2 text-[13px] font-medium text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded-lg transition-colors disabled:opacity-50"
        >
          Cancel
        </button>
        <button
          onClick={onConfirm}
          disabled={busy || nothingToImport}
          className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {busy ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
          Import
        </button>
      </div>
    </div>
  );
}
