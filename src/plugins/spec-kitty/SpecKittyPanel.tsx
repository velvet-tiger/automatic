import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertCircle,
  RefreshCw,
  ScrollText,
  ChevronRight,
} from "lucide-react";

// ── Types ─────────────────────────────────────────────────────────────────────

interface SpecKittyFeatureMeta {
  feature_number: string;
  slug: string;
  friendly_name: string;
  mission: string;
  source_description: string;
  created_at: string;
}

interface SpecKittyWorkPackage {
  id: string;
  title: string;
  lane: "planned" | "doing" | "for_review" | "done" | string;
  phase: string;
  file: string;
  agent?: string;
}

interface SpecKittyFeatureStatus {
  feature: string;
  total_wps: number;
  progress_percentage: number;
  stale_wps: number;
  work_packages: SpecKittyWorkPackage[];
}

// ── Helpers ───────────────────────────────────────────────────────────────────

const LANE_ORDER = ["planned", "doing", "for_review", "done"] as const;

function laneMeta(lane: string): { label: string; dotClass: string; textClass: string } {
  switch (lane) {
    case "planned":    return { label: "Planned",     dotClass: "bg-text-muted", textClass: "text-text-muted" };
    case "doing":      return { label: "In Progress", dotClass: "bg-brand",      textClass: "text-brand" };
    case "for_review": return { label: "For Review",  dotClass: "bg-warning",    textClass: "text-warning" };
    case "done":       return { label: "Done",        dotClass: "bg-success",    textClass: "text-green-400" };
    default:           return { label: lane,          dotClass: "bg-text-muted", textClass: "text-text-muted" };
  }
}

// ── SpecKittyKanban ───────────────────────────────────────────────────────────

function SpecKittyKanban({ status }: { status: SpecKittyFeatureStatus }) {
  const byLane: Record<string, SpecKittyWorkPackage[]> = {};
  for (const lane of LANE_ORDER) byLane[lane] = [];
  for (const wp of status.work_packages) {
    const lane = LANE_ORDER.includes(wp.lane as (typeof LANE_ORDER)[number]) ? wp.lane : "planned";
    (byLane[lane] ??= []).push(wp);
  }

  const activeLanes = LANE_ORDER.filter((lane) => (byLane[lane]?.length ?? 0) > 0);

  if (activeLanes.length === 0) {
    return <p className="text-[12px] text-text-muted italic">No work packages found.</p>;
  }

  return (
    <div className="space-y-3">
      {/* Stats row */}
      <div className="flex items-center gap-4 text-[11px] text-text-muted">
        <span>{status.total_wps} work packages</span>
        <span className="text-green-400">
          {status.work_packages.filter((w) => w.lane === "done").length} done
        </span>
        {status.stale_wps > 0 && (
          <span className="text-warning">{status.stale_wps} stale</span>
        )}
        <span className="ml-auto font-medium text-text-base">
          {Math.round(status.progress_percentage)}% complete
        </span>
      </div>

      {/* Lane columns */}
      <div
        className="grid gap-2"
        style={{ gridTemplateColumns: `repeat(${activeLanes.length}, minmax(0, 1fr))` }}
      >
        {activeLanes.map((lane) => {
          const { label, dotClass, textClass } = laneMeta(lane);
          const wps = byLane[lane] ?? [];
          return (
            <div key={lane} className="flex flex-col gap-1.5">
              <div className="flex items-center gap-1.5 mb-1">
                <div className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${dotClass}`} />
                <span className={`text-[10px] font-semibold uppercase tracking-wider ${textClass}`}>
                  {label}
                </span>
                <span className="text-[10px] text-text-muted ml-auto">{wps.length}</span>
              </div>
              {wps.map((wp) => (
                <div
                  key={wp.id}
                  className="px-2.5 py-2 rounded-md bg-bg-input border border-border-strong/30 hover:border-border-strong/60 transition-colors"
                >
                  <div className="flex items-center gap-1.5 mb-0.5">
                    <span className="text-[10px] font-mono font-semibold text-text-muted">{wp.id}</span>
                    {wp.lane === "doing" && (
                      <span className="w-1.5 h-1.5 rounded-full bg-brand animate-pulse flex-shrink-0" />
                    )}
                  </div>
                  <p className="text-[12px] text-text-base leading-snug">{wp.title}</p>
                  {wp.phase && (
                    <p className="text-[10px] text-text-muted mt-0.5 truncate">{wp.phase}</p>
                  )}
                </div>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── SpecKittyPanel ────────────────────────────────────────────────────────────

export interface SpecKittyPanelProps {
  projectDir: string;
  /** Rendered in the right sidebar slot — pass <ToolInfoSidebar /> from the caller. */
  sidebar: React.ReactNode;
}

export function SpecKittyPanel({ projectDir, sidebar }: SpecKittyPanelProps) {
  const [features, setFeatures] = useState<SpecKittyFeatureMeta[]>([]);
  const [loadingFeatures, setLoadingFeatures] = useState(true);
  const [featuresError, setFeaturesError] = useState<string | null>(null);

  const [statusMap, setStatusMap] = useState<
    Record<string, SpecKittyFeatureStatus | "loading" | "error">
  >({});
  const [expandedSlug, setExpandedSlug] = useState<string | null>(null);

  function loadFeatures() {
    if (!projectDir) { setLoadingFeatures(false); return; }
    setLoadingFeatures(true);
    setFeaturesError(null);
    invoke<SpecKittyFeatureMeta[]>("invoke_tool_command", {
        tool: "spec-kitty",
        command: "list_features",
        payload: { projectDir },
      })
      .then((data) => { setFeatures(data); setLoadingFeatures(false); })
      .catch((err) => { setFeaturesError(String(err)); setLoadingFeatures(false); });
  }

  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { loadFeatures(); }, [projectDir]);

  function refresh() {
    setFeatures([]);
    setStatusMap({});
    setExpandedSlug(null);
    loadFeatures();
  }

  function toggleFeature(slug: string) {
    if (expandedSlug === slug) { setExpandedSlug(null); return; }
    setExpandedSlug(slug);
    if (!statusMap[slug]) {
      setStatusMap((prev) => ({ ...prev, [slug]: "loading" }));
      invoke<SpecKittyFeatureStatus>("invoke_tool_command", {
          tool: "spec-kitty",
          command: "get_status",
          payload: { projectDir, featureSlug: slug },
        })
        .then((data) => setStatusMap((prev) => ({ ...prev, [slug]: data })))
        .catch(() => setStatusMap((prev) => ({ ...prev, [slug]: "error" })));
    }
  }

  return (
    <div className="flex gap-6 items-start">
      {/* ── Main content ── */}
      <section className="flex-1 min-w-0 space-y-4">
        {/* Section header */}
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <ScrollText size={13} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
              Features
            </span>
            {!loadingFeatures && features.length > 0 && (
              <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-bg-input border border-border-strong/40 text-text-muted leading-none">
                {features.length}
              </span>
            )}
          </div>
          <button
            onClick={refresh}
            className="p-1.5 rounded text-text-muted hover:text-text-base hover:bg-bg-input transition-colors"
            title="Refresh features"
          >
            <RefreshCw size={12} />
          </button>
        </div>

        {!projectDir && (
          <p className="text-[12px] text-text-muted italic">
            Set a project directory to load features.
          </p>
        )}

        {projectDir && loadingFeatures && (
          <div className="text-[12px] text-text-muted flex items-center gap-2">
            <RefreshCw size={12} className="animate-spin" /> Loading features…
          </div>
        )}

        {projectDir && !loadingFeatures && featuresError && (
          <div className="text-[12px] text-danger bg-danger/10 border border-danger/20 rounded-lg px-3 py-2">
            {featuresError}
          </div>
        )}

        {projectDir && !loadingFeatures && !featuresError && features.length === 0 && (
          <div className="flex items-start gap-3 px-3 py-3 rounded-lg bg-bg-input border border-border-strong/30 text-[12px] text-text-muted">
            <AlertCircle size={13} className="flex-shrink-0 mt-0.5" />
            <span>
              No features found. Run{" "}
              <code className="font-mono text-[11px] text-text-base">
                spec-kitty specify &lt;feature&gt;
              </code>{" "}
              to create one.
            </span>
          </div>
        )}

        {features.length > 0 && (
          <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
            {features.map((f) => {
              const status = statusMap[f.slug];
              const isExpanded = expandedSlug === f.slug;

              const pct =
                status && status !== "loading" && status !== "error"
                  ? status.progress_percentage
                  : null;
              const totalWps =
                status && status !== "loading" && status !== "error" ? status.total_wps : null;
              const doneWps =
                status && status !== "loading" && status !== "error"
                  ? status.work_packages.filter((w) => w.lane === "done").length
                  : null;

              return (
                <div key={f.slug}>
                  <button
                    onClick={() => toggleFeature(f.slug)}
                    className="w-full flex items-center gap-3 px-4 py-3 hover:bg-surface-hover transition-colors text-left"
                  >
                    <div
                      className={`transition-transform ${isExpanded ? "rotate-90" : ""} text-text-muted`}
                    >
                      <ChevronRight size={14} />
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-[11px] font-mono text-text-muted">
                          #{f.feature_number}
                        </span>
                        <span className="text-[13px] font-medium text-text-base">
                          {f.friendly_name}
                        </span>
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar border border-border-strong/40 text-text-muted">
                          {f.mission}
                        </span>
                      </div>
                      {f.source_description && (
                        <p className="text-[11px] text-text-muted mt-0.5 truncate">
                          {f.source_description}
                        </p>
                      )}
                    </div>
                    <div className="flex items-center gap-2 flex-shrink-0">
                      {status === "loading" && (
                        <RefreshCw size={11} className="text-text-muted animate-spin" />
                      )}
                      {pct !== null && doneWps !== null && totalWps !== null && (
                        <>
                          <span className="text-[11px] text-text-muted tabular-nums">
                            {doneWps}/{totalWps}
                          </span>
                          <div className="w-20 h-1.5 rounded-full bg-border-strong/40 overflow-hidden">
                            <div
                              className="h-full rounded-full bg-green-500 transition-all"
                              style={{ width: `${pct}%` }}
                            />
                          </div>
                          <span className="text-[11px] text-text-muted tabular-nums w-8 text-right">
                            {Math.round(pct)}%
                          </span>
                        </>
                      )}
                    </div>
                  </button>

                  {isExpanded && (
                    <div className="border-t border-border-strong/20 bg-bg-sidebar/50 px-4 py-4">
                      {status === "loading" && (
                        <div className="flex items-center gap-2 text-[12px] text-text-muted py-2">
                          <RefreshCw size={12} className="animate-spin" /> Loading work packages…
                        </div>
                      )}
                      {status === "error" && (
                        <div className="text-[12px] text-danger flex items-center gap-2 py-2">
                          <AlertCircle size={12} /> Failed to load status. Is spec-kitty installed?
                        </div>
                      )}
                      {status && status !== "loading" && status !== "error" && (
                        <SpecKittyKanban status={status} />
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>

      {/* ── Right sidebar slot ── */}
      {sidebar}
    </div>
  );
}
