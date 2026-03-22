/**
 * UpdateContext — background auto-update orchestration.
 *
 * Responsibilities:
 *  - Check for updates once per hour while the window is visible.
 *  - Download the update silently in the background as soon as one is found.
 *  - Expose `status` so the rest of the UI can react (toast, Settings page).
 *  - When a new update arrives while one is already downloaded, discard the
 *    stale pending object and replace it with the new one.
 *  - Expose `checkAndDownload()` so the Settings page can trigger a manual check.
 *  - Expose `restartApp()` to apply the installed update.
 *
 * States:
 *   "idle"        — no check in progress, no update pending
 *   "checking"    — network call to the update endpoint in progress
 *   "downloading" — update found, download in progress
 *   "ready"       — downloaded and installed, restart needed
 *   "up-to-date"  — check completed, already on latest version
 *   "error"       — check or download failed
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { trackUpdateChecked, trackUpdateInstalled } from "../lib/analytics";

export type UpdateStatus =
  | "idle"
  | "checking"
  | "downloading"
  | "ready"
  | "up-to-date"
  | "error";

export interface UpdateInfo {
  version: string;
  notes?: string;
}

interface UpdateContextValue {
  status: UpdateStatus;
  updateInfo: UpdateInfo | null;
  errorMessage: string;
  /** Manually trigger a check + background download. */
  checkAndDownload: () => Promise<void>;
  /** Restart the app to apply an installed update. */
  restartApp: () => void;
}

const CHECK_INTERVAL_MS = 60 * 60 * 1000; // 1 hour

const UpdateContext = createContext<UpdateContextValue | null>(null);

export function useUpdate(): UpdateContextValue {
  const ctx = useContext(UpdateContext);
  if (!ctx) throw new Error("useUpdate must be used inside <UpdateProvider>");
  return ctx;
}

interface UpdateProviderProps {
  children: ReactNode;
}

export function UpdateProvider({ children }: UpdateProviderProps) {
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [errorMessage, setErrorMessage] = useState("");

  // Held in a ref so the interval callback always sees the latest value without
  // triggering an effect re-run.
  const pendingUpdateRef = useRef<Update | null>(null);
  // Guard against concurrent calls (e.g. manual + automatic firing together).
  const checkInProgressRef = useRef(false);

  const checkAndDownload = useCallback(async () => {
    if (checkInProgressRef.current) return;
    checkInProgressRef.current = true;

    setStatus("checking");
    setErrorMessage("");

    try {
      const update = await check();

      if (!update) {
        setStatus("up-to-date");
        trackUpdateChecked("not_available");
        return;
      }

      // A new update is available — discard any previously held pending update
      // and start a fresh download.
      pendingUpdateRef.current = null;
      setUpdateInfo({ version: update.version, notes: update.body ?? undefined });
      trackUpdateChecked("available");

      setStatus("downloading");
      await update.downloadAndInstall();

      // Store the installed update object so Settings can reference it if needed.
      pendingUpdateRef.current = update;
      setStatus("ready");
      trackUpdateInstalled(update.version);
    } catch (e) {
      setErrorMessage(String(e));
      setStatus("error");
      trackUpdateChecked("error");
    } finally {
      checkInProgressRef.current = false;
    }
  }, []);

  const restartApp = useCallback(() => {
    invoke("restart_app");
  }, []);

  // ── Hourly check while the window is visible ──────────────────────────────

  useEffect(() => {
    // Run an initial check shortly after mount (give the app 5 s to settle).
    const initialTimer = setTimeout(() => {
      checkAndDownload();
    }, 5_000);

    // Then check every hour.
    const interval = setInterval(() => {
      // Only run when the document is visible (app window in foreground).
      if (document.visibilityState === "visible") {
        checkAndDownload();
      }
    }, CHECK_INTERVAL_MS);

    return () => {
      clearTimeout(initialTimer);
      clearInterval(interval);
    };
  }, [checkAndDownload]);

  // Also re-check when the window regains visibility, but only if the last
  // successful check was more than an hour ago.
  const lastCheckTimeRef = useRef<number>(0);

  useEffect(() => {
    function handleVisibilityChange() {
      if (document.visibilityState !== "visible") return;
      if (checkInProgressRef.current) return;
      // Skip if we already checked within the last hour.
      if (Date.now() - lastCheckTimeRef.current < CHECK_INTERVAL_MS) return;
      checkAndDownload();
    }

    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () =>
      document.removeEventListener("visibilitychange", handleVisibilityChange);
  }, [checkAndDownload]);

  // Record the time of the last completed check.
  useEffect(() => {
    if (status === "up-to-date" || status === "ready" || status === "error") {
      lastCheckTimeRef.current = Date.now();
    }
  }, [status]);

  return (
    <UpdateContext.Provider
      value={{ status, updateInfo, errorMessage, checkAndDownload, restartApp }}
    >
      {children}
    </UpdateContext.Provider>
  );
}
