import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertCircle, CheckCircle2, RefreshCw, Server, XCircle } from "lucide-react";

import { McpSelector } from "../../../components/McpSelector";
import type { AgentWithProjects } from "../Providers";

// ── Types mirroring the Rust command shapes ──────────────────────────────────

interface GlobalMcpStatus {
  supported: boolean;
  note: string | null;
  target_path: string | null;
  target_exists: boolean;
  reload_note: string | null;
  in_sync: boolean;
  missing: string[];
  skipped: string[];
}

interface EligibleServer {
  name: string;
  eligible: boolean;
  reason: string | null;
}

interface AgentGlobalMcpView {
  selected: string[];
  managed: string[];
  skipped: string[];
  rejected: string[];
  last_applied: string | null;
  supported: boolean;
  target_path: string | null;
  reload_note: string | null;
}

interface GlobalMcpStateResponse {
  agents: Record<string, AgentGlobalMcpView>;
}

interface RejectedServer {
  name: string;
  reason: string;
}

interface GlobalMcpApplyReport {
  path: string;
  written: string[];
  removed: string[];
  skipped: string[];
  unchanged: boolean;
  rejected: RejectedServer[];
  reload_note: string | null;
}

interface GlobalMcpPreview {
  target_path: string;
  target_exists: boolean;
  foreign_entries: string[];
  would_write: string[];
  would_skip: string[];
  would_remove: string[];
  rejected: RejectedServer[];
}

// ── Component ────────────────────────────────────────────────────────────────

interface GlobalMcpTabProps {
  agent: AgentWithProjects;
}

export function GlobalMcpTab({ agent }: GlobalMcpTabProps) {
  const [status, setStatus] = useState<GlobalMcpStatus | null>(null);
  const [view, setView] = useState<AgentGlobalMcpView | null>(null);
  const [eligible, setEligible] = useState<EligibleServer[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastReport, setLastReport] = useState<GlobalMcpApplyReport | null>(null);

  const load = async () => {
    try {
      const [statusRes, stateRaw, eligibleRaw] = await Promise.all([
        invoke<GlobalMcpStatus>("get_global_mcp_status", { agentId: agent.id }),
        invoke<string>("get_global_mcp_state"),
        invoke<string>("list_global_eligible_mcp_servers"),
      ]);
      setStatus(statusRes);
      const parsed = JSON.parse(stateRaw) as GlobalMcpStateResponse;
      setView(parsed.agents[agent.id] ?? null);
      setEligible(JSON.parse(eligibleRaw) as EligibleServer[]);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    load();
    // Loading is keyed by agent id — the drawer swapping agents remounts this.
  }, [agent.id]);

  // ── Unsupported agents get a plain note only ───────────────────────────────
  if (agent.capabilities && !agent.capabilities.global_mcp_servers) {
    const note =
      agent.mcp_note ??
      `${agent.label} does not expose a user-level MCP config file that Automatic can write. Assign MCP servers per project in the Projects tab.`;
    return (
      <section className="max-w-2xl">
        <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3 flex items-center gap-1.5">
          <Server size={12} className="text-text-muted" /> Global MCP
        </label>
        <div className="flex items-start gap-3 px-3 py-3 bg-bg-input rounded-md border border-border-strong">
          <AlertCircle size={14} className="text-text-muted flex-shrink-0 mt-0.5" />
          <p className="text-[12px] text-text-muted leading-relaxed">{note}</p>
        </div>
      </section>
    );
  }

  const selected = view?.selected ?? [];
  const availableServers = eligible.filter((s) => s.eligible).map((s) => s.name);
  const ineligible = eligible.filter((s) => !s.eligible);

  const confirmAndSet = async (nextSelection: string[]) => {
    setBusy(true);
    setError(null);
    try {
      const preview = (await invoke("preview_global_mcp_apply", {
        agentId: agent.id,
        servers: nextSelection,
      })) as GlobalMcpPreview;

      // Confirm on any removal OR on the first write into a file with foreign
      // entries.  Plain adds to an Automatic-created file apply straight away.
      const needsConfirm =
        preview.would_remove.length > 0 ||
        (preview.would_write.length > 0 &&
          preview.target_exists &&
          view != null &&
          view.managed.length === 0 &&
          preview.foreign_entries.length > 0);

      if (needsConfirm) {
        const lines: string[] = [];
        if (preview.would_remove.length > 0) {
          lines.push(`Will remove:`);
          preview.would_remove.forEach((n) => lines.push(`  • ${n}`));
        }
        if (preview.would_write.length > 0) {
          lines.push(preview.would_remove.length ? `` : ``);
          lines.push(`Will write:`);
          preview.would_write.forEach((n) => lines.push(`  • ${n}`));
        }
        if (preview.would_skip.length > 0) {
          lines.push(``);
          lines.push(`Will skip (foreign entries):`);
          preview.would_skip.forEach((n) => lines.push(`  • ${n}`));
        }
        const ok = await ask(
          `Update ${preview.target_path}?\n\n${lines.join("\n")}`,
          { title: `${agent.label} global MCP`, kind: "warning" },
        );
        if (!ok) {
          setBusy(false);
          return;
        }
      }

      const report = (await invoke("set_global_mcp_servers", {
        agentId: agent.id,
        servers: nextSelection,
      })) as GlobalMcpApplyReport;
      setLastReport(report);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onReapply = async () => {
    setBusy(true);
    setError(null);
    try {
      const report = (await invoke("reapply_global_mcp", { agentId: agent.id })) as GlobalMcpApplyReport;
      setLastReport(report);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="max-w-2xl space-y-6">
      <div>
        <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3 flex items-center gap-1.5">
          <Server size={12} className="text-text-muted" /> Global MCP Target
        </label>
        <div className="rounded-lg border border-border-strong/40 bg-bg-input overflow-hidden">
          <div className="flex items-center justify-between gap-3 px-3 py-2.5">
            <div className="min-w-0">
              <div className="text-[11px] text-text-muted uppercase tracking-wider">
                {status?.target_exists ? "File" : "File (will be created)"}
              </div>
              <div className="text-[12px] font-mono text-text-base truncate mt-0.5">
                {status?.target_path ?? "—"}
              </div>
            </div>
            <div className="flex items-center gap-2 flex-shrink-0">
              {status?.in_sync ? (
                <span className="flex items-center gap-1 text-[11px] text-success">
                  <CheckCircle2 size={12} /> In sync
                </span>
              ) : (
                <span className="flex items-center gap-1 text-[11px] text-warning">
                  <AlertCircle size={12} /> Out of sync
                </span>
              )}
              <button
                onClick={onReapply}
                disabled={busy}
                className="flex items-center gap-1 px-2.5 py-1 rounded-md border border-border-strong/40 text-[11px] text-text-base hover:border-border-strong hover:bg-surface-hover transition-colors disabled:opacity-50"
              >
                <RefreshCw size={11} className={busy ? "animate-spin" : ""} /> Re-apply
              </button>
            </div>
          </div>
          {status?.reload_note && (
            <div className="border-t border-border-strong/30 px-3 py-2 text-[11px] text-text-muted leading-relaxed">
              {status.reload_note}
            </div>
          )}
        </div>
      </div>

      {view && view.skipped.length > 0 && (
        <div className="flex items-start gap-3 px-3 py-3 bg-bg-input rounded-md border border-warning/40">
          <AlertCircle size={14} className="text-warning flex-shrink-0 mt-0.5" />
          <div className="text-[12px] text-text-base leading-relaxed">
            <p className="font-medium">Skipped — foreign entries with the same name.</p>
            <p className="text-text-muted mt-1">
              These entries already exist in the target file and were not created by Automatic.
              Rename one side to have Automatic manage them.
            </p>
            <ul className="mt-2 space-y-0.5 font-mono text-[11px]">
              {view.skipped.map((name) => (
                <li key={name}>• {name}</li>
              ))}
            </ul>
          </div>
        </div>
      )}

      {ineligible.length > 0 && (
        <div className="text-[11px] text-text-muted leading-relaxed">
          <div className="uppercase tracking-wider mb-1">Not available at global scope</div>
          <ul className="space-y-0.5">
            {ineligible.map((s) => (
              <li key={s.name} className="font-mono">
                • {s.name}
                <span className="ml-2 not-italic text-text-muted/70">— {s.reason}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {error && (
        <div className="flex items-start gap-3 px-3 py-3 bg-bg-input rounded-md border border-danger/40">
          <XCircle size={14} className="text-danger flex-shrink-0 mt-0.5" />
          <p className="text-[12px] text-danger leading-relaxed">{error}</p>
        </div>
      )}

      {lastReport && !error && (lastReport.written.length > 0 || lastReport.removed.length > 0) && (
        <div className="text-[11px] text-text-muted leading-relaxed">
          Last apply:
          {lastReport.written.length > 0 && (
            <> wrote {lastReport.written.length} {lastReport.written.length === 1 ? "entry" : "entries"}</>
          )}
          {lastReport.removed.length > 0 && (
            <>{lastReport.written.length > 0 ? ", removed" : " removed"} {lastReport.removed.length} {lastReport.removed.length === 1 ? "entry" : "entries"}</>
          )}
          .
        </div>
      )}

      <McpSelector
        servers={selected}
        availableServers={availableServers}
        onAdd={(server) => confirmAndSet([...selected, server])}
        onRemove={(index) => confirmAndSet(selected.filter((_, i) => i !== index))}
        label="Global MCP Servers"
        emptyMessage="No global MCP servers assigned. Add servers from your MCP library to make them available in every chat."
        showRemoveButtonAlways
      />
    </section>
  );
}
