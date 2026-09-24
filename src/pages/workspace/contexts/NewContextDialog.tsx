import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ContextDialog,
  DialogError,
  FIELD_CLASS,
  HINT_CLASS,
  LABEL_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from "./ContextDialog";
import { PAGES_SOURCE_ID, deriveSlug, type Context } from "./types";

/** A slug for `name` that no existing context uses. */
export function uniqueContextSlug(name: string, existing: string[]): string {
  const taken = new Set(existing);
  const base = deriveSlug(name) || "context";
  let candidate = base;
  let i = 2;
  while (taken.has(candidate)) candidate = `${base}-${i++}`;
  return candidate;
}

/**
 * Create a context from just a name, or link one from the Automatic cloud.
 * The context is saved at once so pages and links can be added straight away.
 */
export function NewContextDialog({
  mode,
  existingSlugs,
  onClose,
  onCreated,
}: {
  mode: "local" | "cloud";
  existingSlugs: string[];
  onClose: () => void;
  onCreated: (slug: string) => void;
}) {
  const [name, setName] = useState("");
  const [cloudId, setCloudId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    const display = name.trim();
    if (!display) return setError("Give the context a name.");
    if (mode === "cloud" && !/^ctx_[A-Za-z0-9_-]+$/.test(cloudId)) {
      return setError("Enter the cloud context id. It starts with ctx_.");
    }
    const base = { slug: uniqueContextSlug(display, existingSlugs), display_name: display, description: "", created_at: "", updated_at: "" };
    const context: Context =
      mode === "cloud"
        ? { ...base, location: "cloud", context_id: cloudId }
        : { ...base, location: "local", sources: [{ id: PAGES_SOURCE_ID, kind: "documentation", display_name: "", description: "" }] };
    setBusy(true);
    setError(null);
    try {
      const saved: Context = await invoke("save_context", { context });
      onCreated(saved.slug);
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  return (
    <ContextDialog
      title={mode === "cloud" ? "Link a cloud context" : "New context"}
      onClose={onClose}
      footer={
        <>
          <button onClick={onClose} className={SECONDARY_BUTTON_CLASS}>Cancel</button>
          <button onClick={() => void submit()} disabled={busy} className={PRIMARY_BUTTON_CLASS}>
            {mode === "cloud" ? "Link context" : "Create context"}
          </button>
        </>
      }
    >
      <div>
        <label className={LABEL_CLASS} htmlFor="new-context-name">Name</label>
        <input
          id="new-context-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && mode === "local") void submit(); }}
          placeholder="Coding standards"
          autoFocus
          className={FIELD_CLASS}
        />
        {mode === "local" && <p className={HINT_CLASS}>You can add pages, folders and web pages next.</p>}
      </div>
      {mode === "cloud" && (
        <div>
          <label className={LABEL_CLASS} htmlFor="new-context-cloud">Cloud context id</label>
          <input
            id="new-context-cloud"
            value={cloudId}
            onChange={(e) => setCloudId(e.target.value.trim())}
            onKeyDown={(e) => { if (e.key === "Enter") void submit(); }}
            placeholder="ctx_…"
            className={`${FIELD_CLASS} font-mono`}
          />
          <p className={HINT_CLASS}>Shown on the context's page in the Automatic web app. You need to be signed in.</p>
        </div>
      )}
      <DialogError message={error} />
    </ContextDialog>
  );
}
