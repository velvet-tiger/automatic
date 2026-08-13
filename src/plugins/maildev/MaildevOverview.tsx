import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, ExternalLink, Loader2, Mail, Play, Square, X } from "lucide-react";
import { openExternalUrl } from "../../lib/externalLinks";
import { MAILDEV_ADMIN_URL, type MaildevStatus } from "./types";

const STATUS_POLL_MS = 3000;

interface ToolDetectionEntry {
  name: string;
  detected: boolean | null;
}

export default function MaildevOverview() {
  const [status, setStatus] = useState<MaildevStatus | null>(null);
  const [detected, setDetected] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshStatus = useCallback(async (showSpinner: boolean) => {
    if (showSpinner) setLoading(true);
    try {
      const result = await invoke<MaildevStatus>("get_maildev_status");
      setStatus(result);
      setError(null);
    } catch (err) {
      setError(String(err));
    } finally {
      if (showSpinner) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshStatus(true);
    const interval = setInterval(() => void refreshStatus(false), STATUS_POLL_MS);
    return () => clearInterval(interval);
  }, [refreshStatus]);

  useEffect(() => {
    invoke<ToolDetectionEntry[]>("list_tools_with_detection")
      .then((tools) => {
        const entry = tools.find((t) => t.name === "maildev");
        setDetected(entry?.detected ?? null);
      })
      .catch((err) => setError(String(err)));
  }, []);

  const handleStart = async () => {
    setBusy(true);
    try {
      await invoke("start_maildev");
      await refreshStatus(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleStop = async () => {
    setBusy(true);
    try {
      await invoke("stop_maildev");
      await refreshStatus(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const notInstalled = detected === false;

  return (
    <div className="flex-1 h-full overflow-y-auto custom-scrollbar p-8 bg-bg-base">
      <div className="max-w-4xl mx-auto space-y-6">
        <div className="flex items-start gap-4">
          <div className="p-3 rounded-xl bg-brand/10 border border-brand/20 shrink-0">
            <Mail size={20} className="text-brand" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold text-text-base mb-2">Maildev</h1>
            <p className="text-text-muted text-[13px] leading-relaxed max-w-2xl">
              Catch outgoing SMTP mail in a local inbox during development, with a web UI and
              an MCP server for inspecting captured mail.
            </p>
          </div>
        </div>

        {error && (
          <div className="flex items-center justify-between gap-3 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-[12px]">
            <div className="flex items-center gap-2">
              <AlertCircle size={13} className="shrink-0" />
              <span>{error}</span>
            </div>
            <button onClick={() => setError(null)} className="text-red-400/70 hover:text-red-400">
              <X size={13} />
            </button>
          </div>
        )}

        {notInstalled ? (
          <div className="flex flex-col items-center justify-center py-16 text-center border border-dashed border-border-strong/40 rounded-xl">
            <Mail size={24} className="text-text-muted mb-3" />
            <p className="text-[13px] text-text-base font-medium mb-1">Maildev isn't installed</p>
            <p className="text-[12px] text-text-muted max-w-sm mb-3">
              Install it globally, then reopen this page.
            </p>
            <code className="text-[12px] px-3 py-1.5 rounded-md bg-bg-input border border-border-strong/40 text-text-base font-mono">
              npm install -g maildev
            </code>
            <button
              onClick={() => void openExternalUrl("https://maildev.github.io/maildev/")}
              className="mt-3 flex items-center gap-1 text-[11px] text-brand hover:underline"
            >
              <ExternalLink size={11} />
              Maildev documentation
            </button>
          </div>
        ) : loading ? (
          <div className="flex items-center justify-center py-16 text-text-muted text-[12px]">
            <Loader2 size={16} className="animate-spin mr-2" />
            Loading status…
          </div>
        ) : (
          <div className="rounded-xl border border-border-strong/40 bg-bg-input overflow-hidden">
            <div className="flex items-center gap-3 px-4 py-3">
              <span
                className={`w-2 h-2 rounded-full shrink-0 ${status?.running ? "bg-green-400" : "bg-text-muted/40"}`}
                aria-hidden="true"
              />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-[12.5px] font-medium text-text-base">Maildev</span>
                  <span className={`text-[11px] ${status?.running ? "text-success" : "text-text-muted"}`}>
                    {status?.running ? "Running" : "Stopped"}
                  </span>
                </div>
                {status?.running && status.pid ? (
                  <p className="text-[11px] text-text-muted mt-0.5">
                    pid {status.pid} — started with --mcp
                  </p>
                ) : null}
                {!status?.running && status?.error ? (
                  <p className="text-[11px] text-red-400 mt-0.5 truncate" title={status.error}>
                    {status.error}
                  </p>
                ) : null}
              </div>
              {status?.running && (
                <button
                  onClick={() => void openExternalUrl(MAILDEV_ADMIN_URL)}
                  className="flex items-center gap-1 text-[11px] text-brand hover:underline shrink-0"
                  title={`Open ${MAILDEV_ADMIN_URL}`}
                >
                  <ExternalLink size={11} />
                  Open admin UI
                </button>
              )}
              {busy ? (
                <div className="w-[26px] h-[26px] flex items-center justify-center text-text-muted shrink-0">
                  <Loader2 size={14} className="animate-spin" />
                </div>
              ) : status?.running ? (
                <button
                  onClick={() => void handleStop()}
                  className="w-[26px] h-[26px] flex items-center justify-center rounded-md text-text-muted hover:bg-red-500/10 hover:text-red-400 transition-colors shrink-0"
                  title="Stop"
                  aria-label="Stop Maildev"
                >
                  <Square size={13} />
                </button>
              ) : (
                <button
                  onClick={() => void handleStart()}
                  className="w-[26px] h-[26px] flex items-center justify-center rounded-md text-text-muted hover:bg-green-500/10 hover:text-green-400 transition-colors shrink-0"
                  title="Start"
                  aria-label="Start Maildev"
                >
                  <Play size={13} />
                </button>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
