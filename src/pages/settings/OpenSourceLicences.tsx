import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { FolderOpen, X } from "lucide-react";

/** One licence file, as `read_third_party_licences` returns it. */
interface LicenceFile {
  file_name: string;
  text: string;
}

/** The result of `read_third_party_licences`. `folder` is null in builds without licence files. */
interface ThirdPartyLicences {
  folder: string | null;
  files: LicenceFile[];
}

/** Tab labels for the files release builds ship. The backend returns only these names. */
const TAB_LABELS: Record<string, string> = {
  "rust-crates.txt": "App components",
  "interface.txt": "Interface",
  "system-libraries.txt": "System libraries",
  "THIRD_PARTY_NOTICES.md": "Other notices",
};

type LoadState =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "loaded"; licences: ThirdPartyLicences };

interface OpenSourceLicencesProps {
  onClose: () => void;
}

/**
 * Dialog that shows the third-party licence files shipped in the app bundle,
 * one tab per file. Development builds have none, and the dialog says so.
 */
export default function OpenSourceLicences({ onClose }: OpenSourceLicencesProps) {
  const [state, setState] = useState<LoadState>({ kind: "loading" });
  const [activeFile, setActiveFile] = useState<string | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  // Move focus into the dialog so keyboard and screen-reader users land in it.
  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  useEffect(() => {
    let cancelled = false;
    invoke<ThirdPartyLicences>("read_third_party_licences")
      .then((licences) => {
        if (cancelled) return;
        setState({ kind: "loaded", licences });
        setActiveFile(licences.files[0]?.file_name ?? null);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setState({ kind: "error", message: String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const showFiles = async (folder: string) => {
    setRevealError(null);
    try {
      await revealItemInDir(folder);
    } catch (err: unknown) {
      setRevealError(`Could not show the files: ${String(err)}`);
    }
  };

  const licences = state.kind === "loaded" ? state.licences : null;
  const active = licences?.files.find((f) => f.file_name === activeFile) ?? null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby="open-source-licences-title"
        className="w-full max-w-3xl mx-4 h-[80vh] flex flex-col rounded-xl border border-border-strong/40 bg-bg-input shadow-2xl outline-none"
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40">
          <h3 id="open-source-licences-title" className="text-[14px] font-semibold text-text-base">
            Open-source licences
          </h3>
          <button
            onClick={onClose}
            aria-label="Close"
            className="p-1 rounded text-text-muted hover:text-text-base transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-5 pt-4 text-[13px] text-text-muted leading-relaxed">
          Automatic is built with open-source software. These files list each component and its licence.
        </div>

        {state.kind === "loading" && (
          <p className="px-5 py-5 text-[13px] text-text-muted">Loading licences...</p>
        )}

        {state.kind === "error" && (
          <p className="px-5 py-5 text-[13px] text-danger">Could not read the licence files: {state.message}</p>
        )}

        {licences !== null && licences.files.length === 0 && (
          <p className="px-5 py-5 text-[13px] text-text-muted">
            This build does not include licence files. Release builds include them.
          </p>
        )}

        {licences !== null && licences.files.length > 0 && (
          <>
            <div className="flex items-center justify-between gap-4 px-5 pt-4">
              <div role="tablist" aria-label="Licence files" className="flex flex-wrap gap-1">
                {licences.files.map((file) => {
                  const selected = file.file_name === activeFile;
                  return (
                    <button
                      key={file.file_name}
                      id={`licence-tab-${file.file_name}`}
                      role="tab"
                      aria-selected={selected}
                      aria-controls="licence-tabpanel"
                      onClick={() => setActiveFile(file.file_name)}
                      className={`px-3 py-1.5 rounded-md text-[13px] transition-colors ${
                        selected
                          ? "bg-bg-sidebar text-text-base border border-border-strong/40"
                          : "text-text-muted hover:text-text-base border border-transparent"
                      }`}
                    >
                      {TAB_LABELS[file.file_name] ?? file.file_name}
                    </button>
                  );
                })}
              </div>
              {licences.folder !== null && (
                <button
                  onClick={() => {
                    if (licences.folder !== null) void showFiles(licences.folder);
                  }}
                  className="flex items-center gap-1.5 text-[13px] text-text-muted hover:text-text-base transition-colors shrink-0"
                >
                  <FolderOpen size={14} />
                  Show files
                </button>
              )}
            </div>
            {revealError !== null && <p className="px-5 pt-2 text-[12px] text-danger">{revealError}</p>}
            <div
              id="licence-tabpanel"
              role="tabpanel"
              aria-labelledby={active !== null ? `licence-tab-${active.file_name}` : undefined}
              className="flex-1 min-h-0 mx-5 my-4 overflow-y-auto custom-scrollbar rounded-lg border border-border-strong/40 bg-bg-base">
              <pre className="px-4 py-3 text-[12px] leading-relaxed text-text-base font-mono whitespace-pre-wrap break-words">
                {active?.text ?? ""}
              </pre>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
