import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Terminal, Check, AlertTriangle, RefreshCw } from "lucide-react";

/**
 * Settings → Command Line page.
 *
 * Lets the user install a symlink from a directory on `$PATH` to the bundled
 * `automatic` binary so the CLI is invokable from any shell. The Rust side
 * (`core::cli_install`) owns path selection, status detection, and the
 * actual install/uninstall — this component only renders the result and
 * dispatches the three Tauri commands.
 */

interface CliInstallStatus {
  platform: string;
  binary_path: string;
  install_path: string | null;
  status: "installed" | "stale" | "not_installed" | "unsupported";
  path_hint: string | null;
}

type Busy = "idle" | "loading" | "installing" | "uninstalling";

export default function CommandLineSettings() {
  const [status, setStatus] = useState<CliInstallStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<Busy>("loading");
  const [lastMessage, setLastMessage] = useState<string | null>(null);

  const refresh = async () => {
    setBusy("loading");
    setError(null);
    try {
      const next = await invoke<CliInstallStatus>("cli_install_status");
      setStatus(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy("idle");
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const install = async () => {
    setBusy("installing");
    setError(null);
    setLastMessage(null);
    try {
      const path = await invoke<string>("cli_install_install");
      setLastMessage(`Installed at ${path}`);
      await refresh();
    } catch (e) {
      setError(String(e));
      setBusy("idle");
    }
  };

  const uninstall = async () => {
    setBusy("uninstalling");
    setError(null);
    setLastMessage(null);
    try {
      const message = await invoke<string>("cli_install_uninstall");
      setLastMessage(message);
      await refresh();
    } catch (e) {
      setError(String(e));
      setBusy("idle");
    }
  };

  return (
    <div>
      <h2 className="text-lg font-medium mb-1 text-text-base">Command Line</h2>
      <p className="text-[13px] text-text-muted mb-6">
        Install the <code className="px-1 py-0.5 rounded bg-bg-input text-[12px]">automatic</code> command so you can manage projects, skills, and memory from any terminal. The same binary powers the desktop app and the MCP server.
      </p>

      {error && (
        <div className="mb-6 p-3 rounded-lg border border-danger/40 bg-danger/10 text-[12px] text-danger">
          {error}
        </div>
      )}

      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
        <div className="p-5 flex items-start gap-3">
          <Terminal size={18} className="mt-0.5 text-text-muted shrink-0" />
          <div className="flex-1 min-w-0">
            <StatusLine status={status} busy={busy} />
            {status && (
              <div className="mt-3 space-y-1.5 text-[12px] text-text-muted">
                <DetailRow label="Binary" value={status.binary_path} />
                {status.install_path && (
                  <DetailRow label="Install path" value={status.install_path} />
                )}
                <DetailRow label="Platform" value={status.platform} />
              </div>
            )}
            {status?.path_hint && status.status !== "unsupported" && (
              <div className="mt-4 p-3 rounded border border-warning/30 bg-warning/5 text-[12px] text-text-base flex gap-2">
                <AlertTriangle size={14} className="mt-0.5 text-warning shrink-0" />
                <span className="leading-relaxed">{status.path_hint}</span>
              </div>
            )}
            {lastMessage && (
              <div className="mt-4 p-3 rounded border border-success/30 bg-success/5 text-[12px] text-text-base flex gap-2">
                <Check size={14} className="mt-0.5 text-success shrink-0" />
                <span>{lastMessage}</span>
              </div>
            )}
          </div>
        </div>

        <div className="border-t border-border-strong/40 px-5 py-3 flex items-center gap-2 bg-bg-input-dark/30">
          <ActionButtons
            status={status}
            busy={busy}
            onInstall={install}
            onUninstall={uninstall}
            onRefresh={refresh}
          />
        </div>
      </div>

      <h3 className="text-sm font-medium mt-8 mb-2 text-text-base">Usage</h3>
      <p className="text-[13px] text-text-muted mb-3">
        Once installed, run <code className="px-1 py-0.5 rounded bg-bg-input text-[12px]">automatic --help</code> to see every available command. A few examples:
      </p>
      <pre className="bg-bg-input border border-border-strong/40 rounded-lg p-4 text-[12px] text-text-base overflow-x-auto">
{`automatic projects list
automatic skills search laravel
automatic memory list my-project
automatic projects sync my-project --json`}
      </pre>
    </div>
  );
}

function StatusLine({ status, busy }: { status: CliInstallStatus | null; busy: Busy }) {
  if (busy === "loading" || !status) {
    return <div className="text-[13px] text-text-muted">Checking install status…</div>;
  }
  const label =
    status.status === "installed"
      ? "Installed"
      : status.status === "stale"
        ? "Stale install — points to a different binary"
        : status.status === "unsupported"
          ? "Not supported on this platform"
          : "Not installed";
  const tone =
    status.status === "installed"
      ? "text-success"
      : status.status === "stale"
        ? "text-warning"
        : status.status === "unsupported"
          ? "text-text-muted"
          : "text-text-base";
  return <div className={`text-[13px] font-medium ${tone}`}>{label}</div>;
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex gap-2">
      <span className="shrink-0 w-24 text-text-muted">{label}</span>
      <span className="text-text-base break-all font-mono text-[11.5px]">{value}</span>
    </div>
  );
}

function ActionButtons({
  status,
  busy,
  onInstall,
  onUninstall,
  onRefresh,
}: {
  status: CliInstallStatus | null;
  busy: Busy;
  onInstall: () => void;
  onUninstall: () => void;
  onRefresh: () => void;
}) {
  if (!status || status.status === "unsupported") {
    return (
      <span className="text-[12px] text-text-muted">
        Automatic install is not yet available on this platform.
      </span>
    );
  }

  const installLabel =
    status.status === "installed"
      ? "Reinstall"
      : status.status === "stale"
        ? "Replace"
        : "Install";

  return (
    <>
      <button
        onClick={onInstall}
        disabled={busy !== "idle"}
        className="h-7 px-3 text-[12px] rounded-md bg-brand text-white hover:bg-brand/90 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {busy === "installing" ? "Installing…" : installLabel}
      </button>
      {(status.status === "installed" || status.status === "stale") && (
        <button
          onClick={onUninstall}
          disabled={busy !== "idle"}
          className="h-7 px-3 text-[12px] rounded-md bg-bg-input border border-border-strong/50 text-text-base hover:border-border-strong disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {busy === "uninstalling" ? "Removing…" : "Uninstall"}
        </button>
      )}
      <button
        onClick={onRefresh}
        disabled={busy !== "idle"}
        className="h-7 px-2.5 text-[12px] rounded-md text-text-muted hover:text-text-base hover:bg-surface-hover disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-1.5"
        aria-label="Re-check install status"
        title="Re-check install status"
      >
        <RefreshCw size={12} />
      </button>
    </>
  );
}
