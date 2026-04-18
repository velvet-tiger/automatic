import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  RefreshCw,
  Cloud,
  CheckCircle2,
  AlertTriangle,
  LogIn,
  ArrowUp,
  ArrowDown,
  Trash2,
} from "lucide-react";

/* Cloud library sync page — bidirectional delta sync. The client computes
 * what changed since the last successful sync (upserts + tombstones),
 * uploads that delta, and applies whatever the server sends back from
 * other devices. Secrets in MCP server configs are scrubbed client-side
 * before upload — only key names cross the wire. */

interface AccountProfile {
  user_id: string;
  email: string | null;
  display_name: string;
}

interface AccountStatus {
  signed_in: boolean;
  profile?: AccountProfile;
  webapp_url: string;
}

/** Matches `core::cloud_sync::SyncPreview`. */
interface SyncPreview {
  upsert_count_by_kind: Record<string, number>;
  tombstone_count_by_kind: Record<string, number>;
  total_upserts: number;
  total_tombstones: number;
}

/** Matches `core::cloud_sync::SyncSummary`. Most per-kind fields are `unknown`
 * because the server's response shape is richer than we need for counts. */
interface SyncSummary {
  accepted_upserts: unknown;
  rejected_upserts: unknown;
  applied_tombstones: unknown;
  rejected_tombstones: unknown;
  remote_upsert_count: number;
  remote_tombstone_count: number;
  server_time: string;
}

const KIND_LABELS: Record<string, string> = {
  skill: "Skills",
  rule: "Rules",
  template: "Instructions",
  sub_agent: "Sub-agents",
  command: "Commands",
  mcp_server: "MCP servers",
  collection: "Collections",
  project_template: "Project templates",
};

const KIND_ORDER = [
  "skill",
  "rule",
  "template",
  "sub_agent",
  "command",
  "mcp_server",
  "collection",
  "project_template",
];

function labelForKind(kind: string): string {
  return KIND_LABELS[kind] ?? kind;
}

function countRecords(value: unknown): number {
  if (Array.isArray(value)) return value.length;
  if (value && typeof value === "object") {
    let total = 0;
    for (const inner of Object.values(value as Record<string, unknown>)) {
      if (Array.isArray(inner)) total += inner.length;
    }
    return total;
  }
  return 0;
}

export default function CloudSync() {
  const [accountStatus, setAccountStatus] = useState<AccountStatus | null>(null);
  const [preview, setPreview] = useState<SyncPreview | null>(null);
  const [previewError, setPreviewError] = useState("");
  const [syncing, setSyncing] = useState(false);
  const [syncError, setSyncError] = useState("");
  const [lastSummary, setLastSummary] = useState<SyncSummary | null>(null);
  const [signingIn, setSigningIn] = useState(false);
  const [signInError, setSignInError] = useState("");

  async function refreshAccount() {
    try {
      const status = await invoke<AccountStatus>("account_status");
      setAccountStatus(status);
    } catch (e) {
      console.error("account_status failed", e);
    }
  }

  async function refreshPreview() {
    setPreviewError("");
    try {
      const result = await invoke<SyncPreview>("cloud_build_bundle");
      setPreview(result);
    } catch (e) {
      setPreviewError(String(e));
    }
  }

  useEffect(() => {
    refreshAccount();
    refreshPreview();
  }, []);

  async function handleSignIn() {
    setSigningIn(true);
    setSignInError("");
    try {
      await invoke<AccountProfile>("account_login");
      await refreshAccount();
    } catch (e) {
      setSignInError(String(e));
    } finally {
      setSigningIn(false);
    }
  }

  async function handleSync() {
    setSyncing(true);
    setSyncError("");
    setLastSummary(null);
    try {
      const summary = await invoke<SyncSummary>("cloud_sync_library");
      setLastSummary(summary);
      await refreshPreview();
    } catch (e) {
      setSyncError(String(e));
    } finally {
      setSyncing(false);
    }
  }

  const signedIn = accountStatus?.signed_in === true;
  const hasDelta =
    preview !== null &&
    preview.total_upserts + preview.total_tombstones > 0;

  return (
    <div className="flex flex-1 h-full bg-bg-base overflow-hidden text-text-base">
      <div className="flex-1 overflow-y-auto h-full">
        <div className="p-8 max-w-3xl">
          <div className="flex items-center gap-2 mb-1">
            <Cloud size={18} className="text-text-base" />
            <h1 className="text-lg font-medium text-text-base">Cloud Sync</h1>
          </div>
          <p className="text-[13px] text-text-muted mb-6 leading-relaxed">
            Sync your skills, rules, templates, sub-agents, commands, MCP
            configs and collections across devices via your Automatic account
            on{" "}
            <span className="font-mono text-[12px] text-text-base">
              {accountStatus?.webapp_url || "tryautomatic.app"}
            </span>
            . Secrets in MCP server configs stay on your machine — only the
            key names are uploaded.
          </p>

          {/* ── Sign-in gate ─────────────────────────────────────────── */}
          {!signedIn && (
            <div className="p-4 rounded-lg border border-border-strong/40 bg-bg-input mb-6">
              <div className="flex items-start gap-3">
                <LogIn size={16} className="text-text-muted mt-0.5" />
                <div className="flex-1">
                  <div className="text-[13px] font-medium text-text-base mb-1">
                    Sign in to enable cloud sync
                  </div>
                  <p className="text-[12px] text-text-muted mb-3 leading-relaxed">
                    Cloud sync requires a signed-in Automatic account. Signing
                    in opens your browser to authorise this device.
                  </p>
                  <button
                    onClick={handleSignIn}
                    disabled={signingIn}
                    className="px-3 py-1.5 rounded-lg bg-brand hover:bg-brand-hover text-white text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {signingIn ? "Waiting for browser…" : "Sign in"}
                  </button>
                  {signInError && (
                    <div className="mt-3 text-[12px] text-danger">{signInError}</div>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* ── Outgoing delta preview ───────────────────────────────── */}
          <div className="p-4 rounded-lg border border-border-strong/40 bg-bg-input mb-6">
            <div className="flex items-center justify-between mb-3">
              <div>
                <div className="text-[13px] font-medium text-text-base">
                  Pending changes
                </div>
                <div className="text-[11px] text-text-muted">
                  {preview
                    ? hasDelta
                      ? `${preview.total_upserts} upload${preview.total_upserts === 1 ? "" : "s"}, ${preview.total_tombstones} deletion${preview.total_tombstones === 1 ? "" : "s"}`
                      : "Up to date — no local changes since the last sync"
                    : "Calculating…"}
                </div>
              </div>
              <button
                onClick={refreshPreview}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-border-strong/40 bg-bg-input-dark text-[11px] text-text-muted hover:text-text-base hover:border-border-strong transition-all"
                title="Recalculate delta"
              >
                <RefreshCw size={12} />
                Refresh
              </button>
            </div>

            {previewError ? (
              <div className="p-3 rounded-md border border-danger bg-danger/10 text-[12px] text-danger flex items-start gap-2">
                <AlertTriangle size={14} className="mt-0.5" />
                <span>{previewError}</span>
              </div>
            ) : preview ? (
              <ul className="space-y-1">
                {KIND_ORDER.map((kind) => {
                  const upserts = preview.upsert_count_by_kind[kind] ?? 0;
                  const tombstones = preview.tombstone_count_by_kind[kind] ?? 0;
                  if (upserts === 0 && tombstones === 0) return null;
                  return (
                    <li
                      key={kind}
                      className="flex items-center justify-between text-[12px]"
                    >
                      <span className="text-text-muted">
                        {labelForKind(kind)}
                      </span>
                      <span className="flex items-center gap-3 font-mono text-text-base">
                        {upserts > 0 && (
                          <span
                            className="flex items-center gap-1"
                            title={`${upserts} new or edited`}
                          >
                            <ArrowUp size={11} className="text-success" />
                            {upserts}
                          </span>
                        )}
                        {tombstones > 0 && (
                          <span
                            className="flex items-center gap-1"
                            title={`${tombstones} deleted locally`}
                          >
                            <Trash2 size={11} className="text-danger" />
                            {tombstones}
                          </span>
                        )}
                      </span>
                    </li>
                  );
                })}
                {!hasDelta && (
                  <li className="text-[12px] text-text-muted">
                    Your library matches what the server last saw.
                  </li>
                )}
              </ul>
            ) : (
              <div className="text-[12px] text-text-muted">Calculating…</div>
            )}
          </div>

          {/* ── Sync action ──────────────────────────────────────────── */}
          <div className="p-4 rounded-lg border border-border-strong/40 bg-bg-input mb-6">
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <div className="text-[13px] font-medium text-text-base mb-1">
                  Sync with cloud
                </div>
                <p className="text-[12px] text-text-muted leading-relaxed">
                  Uploads your local changes and pulls anything new from other
                  devices. Last-writer-wins resolves any conflicts by timestamp.
                </p>
              </div>
              <button
                onClick={handleSync}
                disabled={!signedIn || syncing || preview === null}
                className="shrink-0 flex items-center gap-1.5 px-3 py-2 rounded-lg bg-brand hover:bg-brand-hover text-white text-[13px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <RefreshCw size={14} className={syncing ? "animate-spin" : ""} />
                {syncing ? "Syncing…" : "Sync now"}
              </button>
            </div>
            {syncError && (
              <div className="mt-3 p-3 rounded-md border border-danger bg-danger/10 text-[12px] text-danger flex items-start gap-2">
                <AlertTriangle size={14} className="mt-0.5" />
                <span className="break-words">{syncError}</span>
              </div>
            )}
          </div>

          {/* ── Last summary ─────────────────────────────────────────── */}
          {lastSummary && (
            <div className="p-4 rounded-lg border border-border-strong/40 bg-bg-input">
              <div className="flex items-center gap-2 mb-3">
                <CheckCircle2 size={14} className="text-success" />
                <div className="text-[13px] font-medium text-text-base">
                  Last sync succeeded
                </div>
                <span className="ml-auto text-[11px] text-text-muted font-mono">
                  {lastSummary.server_time}
                </span>
              </div>
              <ul className="space-y-1 text-[12px]">
                <SummaryRow
                  icon={<ArrowUp size={12} className="text-success" />}
                  label="Uploaded and accepted"
                  count={countRecords(lastSummary.accepted_upserts)}
                />
                <SummaryRow
                  icon={<ArrowUp size={12} className="text-text-muted" />}
                  label="Uploaded but overwritten (server had newer)"
                  count={countRecords(lastSummary.rejected_upserts)}
                />
                <SummaryRow
                  icon={<Trash2 size={12} className="text-danger" />}
                  label="Deletions applied on server"
                  count={countRecords(lastSummary.applied_tombstones)}
                />
                <SummaryRow
                  icon={<Trash2 size={12} className="text-text-muted" />}
                  label="Deletions rejected (resurrected elsewhere)"
                  count={countRecords(lastSummary.rejected_tombstones)}
                />
                <SummaryRow
                  icon={<ArrowDown size={12} className="text-success" />}
                  label="Received from other devices"
                  count={lastSummary.remote_upsert_count}
                />
                <SummaryRow
                  icon={<ArrowDown size={12} className="text-danger" />}
                  label="Deleted by other devices"
                  count={lastSummary.remote_tombstone_count}
                />
              </ul>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function SummaryRow({
  icon,
  label,
  count,
}: {
  icon: React.ReactNode;
  label: string;
  count: number;
}) {
  if (count === 0) return null;
  return (
    <li className="flex items-center justify-between text-text-base">
      <span className="flex items-center gap-2">
        {icon}
        <span className="text-text-muted">{label}</span>
      </span>
      <span className="font-mono">{count}</span>
    </li>
  );
}
