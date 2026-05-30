// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, Plus, RefreshCw } from "lucide-react";
import { formatAssetScanResult, scanAssetContent, warningFindings } from "../../../../lib/assetSecurity";
import type { ProjectRecommendation } from "../types";

/**
 * Installs an AI-suggested skill from the remote registry and then notifies
 * the parent to add it to the project config.
 *
 * Error handling is explicit: on failure the button shows an error message and
 * nothing is added to the project — no broken references are created.
 */
export function SkillAddButton({
  rec,
  alreadyAdded,
  onAdd,
}: {
  rec: ProjectRecommendation;
  alreadyAdded: boolean;
  onAdd: (skillName: string) => Promise<boolean> | boolean;
}) {
  const [state, setState] = useState<"idle" | "loading" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [noticeMsg, setNoticeMsg] = useState<string | null>(null);

  const handleAdd = async () => {
    setState("loading");
    setErrorMsg(null);
    setNoticeMsg(null);

    // 1. Resolve skill metadata from the stored blob or by searching.
    let meta: { id: string; name: string; source: string } | null = null;
    if (rec.metadata) {
      try {
        const parsed = JSON.parse(rec.metadata) as { id: string; name: string; source: string };
        if (parsed.name && parsed.source) meta = parsed;
      } catch { /* fall through to search */ }
    }
    if (!meta) {
      try {
        const results = await invoke<{ id: string; name: string; source: string; installs: number }[]>(
          "search_remote_skills", { query: rec.title },
        );
        const match = results.find((r) => r.name === rec.title) ?? results[0];
        if (match) meta = { id: match.id, name: match.name, source: match.source };
      } catch { /* search failed */ }
    }

    if (!meta) {
      setState("error");
      setErrorMsg("Could not find this skill in the registry.");
      return;
    }

    // 2. Fetch the skill content from the remote registry.
    let content: string;
    try {
      content = await invoke("fetch_remote_skill_content", { source: meta.source, name: meta.name });
    } catch (err: any) {
      setState("error");
      setErrorMsg(`Failed to fetch skill: ${err}`);
      return;
    }

    // 3. Install it locally.
    try {
      const scan = await scanAssetContent("skill", content);
      if (scan.blocked) {
        setState("error");
        setErrorMsg(formatAssetScanResult(scan, "skill"));
        return;
      }
      await invoke("import_remote_skill", { name: meta.name, content, source: meta.source, id: meta.id });
      const warnings = warningFindings(scan);
      setNoticeMsg(warnings.length > 0 ? formatAssetScanResult(scan, "skill") : null);
    } catch (err: any) {
      setState("error");
      setErrorMsg(`Failed to install skill: ${err}`);
      return;
    }

    // 4. Everything succeeded — add to project and dismiss the card.
    setState("idle");
    const added = await Promise.resolve(onAdd(meta.name));
    if (!added) {
      setState("error");
      setErrorMsg("Installed skill, but failed to add it to this project.");
      return;
    }
  };

  if (errorMsg) {
    return (
      <span className="text-[11px] text-error flex items-center gap-1 whitespace-pre-wrap">
        <AlertCircle size={10} className="mt-0.5 flex-shrink-0" /> {errorMsg}
      </span>
    );
  }

  if (noticeMsg) {
    return (
      <span className="text-[11px] text-amber-950 flex items-center gap-1 whitespace-pre-wrap">
        <AlertCircle size={10} className="mt-0.5 flex-shrink-0" /> {noticeMsg}
      </span>
    );
  }

  return (
    <button
      onClick={handleAdd}
      disabled={alreadyAdded || state === "loading"}
      className="text-[11px] font-medium text-brand hover:text-brand-hover border border-brand/40 rounded px-2 py-1 transition-colors flex items-center gap-1 disabled:opacity-40 disabled:cursor-default"
    >
      {state === "loading"
        ? <><RefreshCw size={10} className="animate-spin" /> Installing…</>
        : <><Plus size={10} /> {alreadyAdded ? "Added" : "Add to project"}</>
      }
    </button>
  );
}
