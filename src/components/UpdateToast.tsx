/**
 * UpdateToast — slim banner shown when a background update is ready to apply.
 *
 * Renders inside the main content area (App.tsx) so it is always visible
 * regardless of which tab the user is on. Dismissed automatically when the
 * user restarts or when status changes away from "ready".
 */
import { RefreshCw } from "lucide-react";
import { useUpdate } from "../contexts/UpdateContext";

export default function UpdateToast() {
  const { status, updateInfo, restartApp } = useUpdate();

  if (status !== "ready") return null;

  return (
    <div className="flex items-center gap-3 px-4 py-2.5 bg-brand/15 border-b border-brand/30 text-[13px]">
      <RefreshCw size={13} className="text-brand flex-shrink-0" />
      <span className="text-text-base flex-1">
        {updateInfo
          ? `Automatic ${updateInfo.version} is ready to install.`
          : "An update is ready to install."}
      </span>
      <button
        onClick={restartApp}
        className="flex-shrink-0 px-3 py-1 rounded text-[12px] font-medium bg-brand text-white hover:bg-brand-hover transition-colors"
      >
        Restart Now
      </button>
    </div>
  );
}
