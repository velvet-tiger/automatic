import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, ArrowRight, ChevronDown, ChevronRight, Code, FileText, FolderOpen, Layers, Lightbulb, RefreshCw, Server, Sparkles, X } from "lucide-react";

interface RecRowProps {
  icon: React.ReactNode;
  title: string;
  badge?: React.ReactNode;
  body: string;
  linkLabel: React.ReactNode;
  onLinkClick: () => void;
  secondaryLinkLabel?: React.ReactNode;
  onSecondaryLinkClick?: () => void;
  onDismiss?: () => void;
}

function RecRow({ icon, title, badge, body, linkLabel, onLinkClick, secondaryLinkLabel, onSecondaryLinkClick, onDismiss }: RecRowProps) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div className="group/row hover:bg-surface-hover transition-colors">
      <div className="flex items-center gap-2 px-3 py-2">
        <button
          onClick={() => setExpanded((v) => !v)}
          className="flex items-center gap-2 flex-1 min-w-0 text-left"
        >
          {expanded
            ? <ChevronDown size={12} className="flex-shrink-0 text-text-muted" />
            : <ChevronRight size={12} className="flex-shrink-0 text-text-muted" />}
          <span className="flex-shrink-0">{icon}</span>
          <span className="text-[13px] font-medium text-text-base truncate">{title}</span>
          {badge && <span className="flex-shrink-0">{badge}</span>}
        </button>
        <button
          onClick={onLinkClick}
          className="flex-shrink-0 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
        >
          {linkLabel}
        </button>
        {secondaryLinkLabel && onSecondaryLinkClick && (
          <button
            onClick={onSecondaryLinkClick}
            className="flex-shrink-0 text-[11px] text-text-muted hover:text-text-base transition-colors font-medium flex items-center gap-1"
          >
            {secondaryLinkLabel}
          </button>
        )}
        {onDismiss && (
          <button
            onClick={onDismiss}
            className="flex-shrink-0 p-0.5 text-text-muted hover:text-text-base transition-colors opacity-0 group-hover/row:opacity-100"
            title="Dismiss"
          >
            <X size={12} />
          </button>
        )}
      </div>
      {expanded && (
        <p className="px-9 pb-2.5 text-[12px] text-text-muted leading-relaxed">{body}</p>
      )}
    </div>
  );
}

interface Recommendation {
  id: number;
  project: string;
  kind: string;
  title: string;
  body: string;
  priority: "low" | "normal" | "high";
  status: "pending" | "dismissed" | "actioned";
  source: string;
  metadata: string;
  created_at: string;
  updated_at: string;
}

interface RecommendationsProps {
  onNavigateToProject: (name: string, tab?: string) => void;
  onNavigateToSkillStoreWithResult?: (result: { id: string; name: string; source: string; installs: number }) => void;
  onNavigateToMcpMarketplace?: (slug: string) => void;
  onNavigateToTemplateMarketplace?: (templateName: string) => void;
  onNavigateToCollectionMarketplace?: (query: string) => void;
}

/** Sources whose individual records are replaced by a single rollup card. */
const AI_SUGGESTION_SOURCES = new Set(["automatic-ai-skills", "automatic-ai-mcp"]);

export default function Recommendations({
  onNavigateToProject,
  onNavigateToSkillStoreWithResult,
  onNavigateToMcpMarketplace,
  onNavigateToTemplateMarketplace,
  onNavigateToCollectionMarketplace,
}: RecommendationsProps) {
  const [recommendations, setRecommendations] = useState<Recommendation[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const recs = await invoke<Recommendation[]>("list_all_pending_recommendations");
      setRecommendations(recs);
    } catch (e) {
      console.error("Failed to load recommendations:", e);
      setRecommendations([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  // Re-load whenever Projects.tsx re-evaluates recommendations after a save.
  useEffect(() => {
    const handler = () => { load(); };
    window.addEventListener("recommendations-updated", handler);
    return () => window.removeEventListener("recommendations-updated", handler);
  }, [load]);

  const handleDismiss = async (id: number) => {
    try {
      await invoke("dismiss_recommendation", { id });
      setRecommendations((prev) => prev.filter((r) => r.id !== id));
    } catch (e) {
      console.error("Failed to dismiss recommendation:", e);
    }
  };

  const handleProjectClick = (name: string, tab?: string) => {
    localStorage.setItem("automatic.projects.selected", name);
    onNavigateToProject(name, tab);
  };

  const getMarketplaceLink = (rec: Recommendation): { label: React.ReactNode; onClick: () => void } | null => {
    let parsedMeta: Record<string, unknown> | null = null;
    if (rec.metadata) {
      try {
        parsedMeta = JSON.parse(rec.metadata) as Record<string, unknown>;
      } catch {
        parsedMeta = null;
      }
    }

    if (rec.kind === "skill" && onNavigateToSkillStoreWithResult && parsedMeta) {
      const id = typeof parsedMeta.id === "string" ? parsedMeta.id : "";
      const name = typeof parsedMeta.name === "string" ? parsedMeta.name : "";
      const source = typeof parsedMeta.source === "string" ? parsedMeta.source : "";
      const installs = typeof parsedMeta.installs === "number" ? parsedMeta.installs : 0;
      if (!id || !name || !source) return null;

      return {
        label: <><Code size={10} /> View skill</>,
        onClick: () => onNavigateToSkillStoreWithResult({ id, name, source, installs }),
      };
    }

    if (rec.kind === "mcp_server" && onNavigateToMcpMarketplace && parsedMeta) {
      const slug = typeof parsedMeta.slug === "string" ? parsedMeta.slug : "";
      if (!slug) return null;
      return {
        label: <><Server size={10} /> View MCP</>,
        onClick: () => onNavigateToMcpMarketplace(slug),
      };
    }

    if (rec.kind === "template" && onNavigateToTemplateMarketplace && parsedMeta) {
      const name = typeof parsedMeta.name === "string" ? parsedMeta.name : "";
      if (!name) return null;
      return {
        label: <><FileText size={10} /> View template</>,
        onClick: () => onNavigateToTemplateMarketplace(name),
      };
    }

    if (rec.kind === "collection" && onNavigateToCollectionMarketplace && parsedMeta) {
      const slug = typeof parsedMeta.slug === "string" ? parsedMeta.slug : "";
      const name = typeof parsedMeta.name === "string" ? parsedMeta.name : "";
      const query = slug || name;
      if (!query) return null;
      return {
        label: <><Layers size={10} /> View collection</>,
        onClick: () => onNavigateToCollectionMarketplace(query),
      };
    }

    return null;
  };

  // Separate normal recs from AI suggestion rollup sources, grouped by project.
  const groupedNormal = recommendations
    .filter((r) => !AI_SUGGESTION_SOURCES.has(r.source))
    .reduce<Map<string, Recommendation[]>>((map, rec) => {
      const list = map.get(rec.project) ?? [];
      list.push(rec);
      map.set(rec.project, list);
      return map;
    }, new Map());

  // Build rollup summaries: for each project, count AI-suggested skills and MCP servers.
  const aiRollupByProject = recommendations
    .filter((r) => AI_SUGGESTION_SOURCES.has(r.source))
    .reduce<Map<string, { skillCount: number; mcpCount: number }>>((map, rec) => {
      const entry = map.get(rec.project) ?? { skillCount: 0, mcpCount: 0 };
      if (rec.source === "automatic-ai-skills") entry.skillCount++;
      if (rec.source === "automatic-ai-mcp") entry.mcpCount++;
      map.set(rec.project, entry);
      return map;
    }, new Map());

  // Merge: all projects that have either normal recs or AI rollups.
  const allProjects = new Set([...groupedNormal.keys(), ...aiRollupByProject.keys()]);

  // Total visible count: normal recs + one rollup card per category per project
  const totalCount = recommendations.filter((r) => !AI_SUGGESTION_SOURCES.has(r.source)).length
    + [...aiRollupByProject.values()].reduce((n, v) => n + (v.skillCount > 0 ? 1 : 0) + (v.mcpCount > 0 ? 1 : 0), 0);

  return (
    <div className="flex-1 h-full overflow-y-auto custom-scrollbar bg-bg-base">
      {/* Top bar */}
      <div className="h-11 px-6 border-b border-border-strong/40 flex items-center justify-between bg-bg-base/50 flex-shrink-0">
        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
          Recommendations
        </span>
        <div className="flex items-center gap-2">
          {totalCount > 0 && (
            <span className="text-[11px] font-semibold px-1.5 py-0.5 rounded-full bg-warning/15 text-warning border border-warning/20 leading-none">
              {totalCount}
            </span>
          )}
          <button
            onClick={load}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded-md disabled:opacity-40 transition-colors"
          >
            <RefreshCw size={11} className={loading ? "animate-spin" : ""} />
            Refresh
          </button>
        </div>
      </div>

      <div className="p-6">
        {loading ? (
          <div className="flex items-center justify-center py-24 text-text-muted">
            <RefreshCw size={16} className="animate-spin mr-2" />
            <span className="text-[13px]">Loading recommendations…</span>
          </div>
        ) : allProjects.size === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <div className="w-16 h-16 rounded-2xl border border-dashed border-border-strong flex items-center justify-center mb-5">
              <Lightbulb size={24} className="text-text-muted" />
            </div>
            <h2 className="text-[16px] font-semibold text-text-base mb-2">No recommendations at this time</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs">
              Open a project to evaluate its configuration. Recommendations will appear here when improvements are found.
            </p>
          </div>
        ) : (
          <div className="space-y-6 max-w-2xl">
            {[...allProjects].map((projectName) => {
              const normalRecs = groupedNormal.get(projectName) ?? [];
              const aiRollup = aiRollupByProject.get(projectName);

              return (
                <section key={projectName}>
                  {/* Project heading */}
                  <button
                    onClick={() => handleProjectClick(projectName)}
                    className="flex items-center gap-2 mb-3 group"
                  >
                    <FolderOpen size={13} className="text-text-muted group-hover:text-brand transition-colors" />
                    <span className="text-[13px] font-semibold text-text-base group-hover:text-brand transition-colors">
                      {projectName}
                    </span>
                    <ArrowRight size={11} className="text-text-muted opacity-0 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all" />
                  </button>

                  <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                    {/* AI skills rollup */}
                    {aiRollup && aiRollup.skillCount > 0 && (
                      <RecRow
                        icon={<Sparkles size={13} className="text-brand" />}
                        title={`${aiRollup.skillCount} skill${aiRollup.skillCount !== 1 ? "s" : ""} recommended`}
                        badge={<span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-brand/10 text-brand border border-brand/20 leading-none flex items-center gap-1"><Sparkles size={8} /> AI</span>}
                        body={`The AI has identified ${aiRollup.skillCount} skill${aiRollup.skillCount !== 1 ? "s" : ""} that may benefit this project. Open the project and go to the Skills tab to review and add them.`}
                        linkLabel={<><Code size={10} /> Open project → Skills tab</>}
                        onLinkClick={() => handleProjectClick(projectName, "skills")}
                      />
                    )}

                    {/* AI MCP rollup */}
                    {aiRollup && aiRollup.mcpCount > 0 && (
                      <RecRow
                        icon={<Sparkles size={13} className="text-brand" />}
                        title={`${aiRollup.mcpCount} MCP server${aiRollup.mcpCount !== 1 ? "s" : ""} recommended`}
                        badge={<span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-brand/10 text-brand border border-brand/20 leading-none flex items-center gap-1"><Sparkles size={8} /> AI</span>}
                        body={`The AI has identified ${aiRollup.mcpCount} MCP server${aiRollup.mcpCount !== 1 ? "s" : ""} that may benefit this project. Open the project and go to the MCP Servers tab to review and add them.`}
                        linkLabel={<><Server size={10} /> Open project → MCP Servers tab</>}
                        onLinkClick={() => handleProjectClick(projectName, "mcp_servers")}
                      />
                    )}

                    {/* Normal recommendation cards */}
                    {normalRecs.map((rec) => {
                      const marketplaceLink = getMarketplaceLink(rec);
                      return (
                        <RecRow
                          key={rec.id}
                          icon={<AlertCircle size={13} className={rec.priority === "high" ? "text-warning" : "text-text-muted"} />}
                          title={rec.title}
                          badge={rec.priority === "high" ? (
                            <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/20 leading-none shrink-0">Important</span>
                          ) : undefined}
                          body={rec.body}
                          linkLabel={<>Open project <ArrowRight size={10} /></>}
                          onLinkClick={() => handleProjectClick(projectName, "recommendations")}
                          secondaryLinkLabel={marketplaceLink?.label}
                          onSecondaryLinkClick={marketplaceLink?.onClick}
                          onDismiss={() => handleDismiss(rec.id)}
                        />
                      );
                    })}
                  </div>
                </section>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
