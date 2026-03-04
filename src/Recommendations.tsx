import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, ArrowRight, FolderOpen, Lightbulb, RefreshCw, X } from "lucide-react";

interface Recommendation {
  id: number;
  project: string;
  kind: string;
  title: string;
  body: string;
  priority: "low" | "normal" | "high";
  status: "pending" | "dismissed" | "actioned";
  source: string;
  created_at: string;
  updated_at: string;
}

interface RecommendationsProps {
  onNavigateToProject: (name: string) => void;
}

export default function Recommendations({ onNavigateToProject }: RecommendationsProps) {
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

  const handleDismiss = async (id: number) => {
    try {
      await invoke("dismiss_recommendation", { id });
      setRecommendations((prev) => prev.filter((r) => r.id !== id));
    } catch (e) {
      console.error("Failed to dismiss recommendation:", e);
    }
  };

  const handleProjectClick = (name: string) => {
    localStorage.setItem("automatic.projects.selected", name);
    onNavigateToProject(name);
  };

  // Group by project, preserving the priority-first ordering from the backend.
  const grouped = recommendations.reduce<Map<string, Recommendation[]>>((map, rec) => {
    const list = map.get(rec.project) ?? [];
    list.push(rec);
    map.set(rec.project, list);
    return map;
  }, new Map());

  return (
    <div className="flex-1 h-full overflow-y-auto custom-scrollbar bg-bg-base">
      {/* Top bar */}
      <div className="h-11 px-6 border-b border-border-strong/40 flex items-center justify-between bg-bg-base/50 flex-shrink-0">
        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
          Recommendations
        </span>
        <div className="flex items-center gap-2">
          {recommendations.length > 0 && (
            <span className="text-[11px] font-semibold px-1.5 py-0.5 rounded-full bg-warning/15 text-warning border border-warning/20 leading-none">
              {recommendations.length}
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
        ) : recommendations.length === 0 ? (
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
            {[...grouped.entries()].map(([projectName, recs]) => (
              <section key={projectName}>
                {/* Project heading with link */}
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

                {/* Recommendation cards */}
                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                  {recs.map((rec) => (
                    <div
                      key={rec.id}
                      className="flex items-start gap-3 px-4 py-4 group/row hover:bg-surface-hover transition-colors"
                    >
                      <AlertCircle
                        size={14}
                        className={`flex-shrink-0 mt-0.5 ${rec.priority === "high" ? "text-warning" : "text-text-muted"}`}
                      />
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-1">
                          <span className="text-[13px] font-semibold text-text-base">{rec.title}</span>
                          {rec.priority === "high" && (
                            <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/20 leading-none shrink-0">
                              Important
                            </span>
                          )}
                        </div>
                        <p className="text-[12px] text-text-muted leading-relaxed">{rec.body}</p>
                        <button
                          onClick={() => handleProjectClick(projectName)}
                          className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                        >
                          Open project <ArrowRight size={10} />
                        </button>
                      </div>
                      <button
                        onClick={() => handleDismiss(rec.id)}
                        className="flex-shrink-0 p-1 text-text-muted hover:text-text-base transition-colors opacity-0 group-hover/row:opacity-100"
                        title="Dismiss"
                      >
                        <X size={13} />
                      </button>
                    </div>
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
