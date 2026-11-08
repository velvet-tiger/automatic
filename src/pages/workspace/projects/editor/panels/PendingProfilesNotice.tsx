import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Project, ProjectProfile, ProfileResourceKind } from "../../types";
import { PROFILE_RESOURCE_KINDS } from "../../types";
import { KIND_LABELS } from "./ProfilesPanel";
import { NOTICE_PRIMARY_BUTTON, NOTICE_SECONDARY_BUTTON, NoticeRow } from "./NoticeRow";

interface PendingProfilesNoticeProps {
  project: Project;
  /** local_key of the open project. */
  selectedKey: string;
  reloadProject: (projectKey: string) => Promise<void>;
}

function projectEntries(project: Project, kind: ProfileResourceKind): string[] {
  if (kind === "rules") return project.file_rules?.["_project"] ?? [];
  return project[kind] ?? [];
}

/** One "2 skills: a, b" part per kind, for what the project does not have yet. */
export function describeAdditions(project: Project, profile: ProjectProfile): string[] {
  const parts: string[] = [];
  for (const kind of PROFILE_RESOURCE_KINDS) {
    const existing = projectEntries(project, kind);
    const same = (a: string, b: string) =>
      kind === "mcp_servers" ? a.toLowerCase() === b.toLowerCase() : a === b;
    const added = (profile[kind] ?? []).filter((item) => !existing.some((e) => same(e, item)));
    if (added.length === 0) continue;
    const [one, many] = KIND_LABELS[kind];
    parts.push(`${added.length} ${added.length === 1 ? one : many}: ${added.join(", ")}`);
  }
  return parts;
}

/**
 * Asks about profiles the project's committed config names but this checkout
 * has not accepted. Nothing from them applies until the user clicks Attach.
 */
export function PendingProfilesNotice({ project, selectedKey, reloadProject }: PendingProfilesNoticeProps) {
  const ignored = project.local_ignored_profiles ?? [];
  const waiting = (project.pending_profiles ?? []).filter((name) => !ignored.includes(name));
  const waitingKey = waiting.join("\n");
  const [profiles, setProfiles] = useState<Record<string, ProjectProfile>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const names = waitingKey ? waitingKey.split("\n") : [];
    void Promise.all(
      names.map(async (name) => {
        try {
          const raw: string = await invoke("read_project_profile", { name });
          return [name, JSON.parse(raw) as ProjectProfile] as const;
        } catch (err) {
          setError(`Could not read the profile "${name}": ${err}`);
          return null;
        }
      }),
    ).then((loaded) => {
      if (cancelled) return;
      setProfiles(Object.fromEntries(loaded.filter((entry) => entry !== null)));
    });
    return () => {
      cancelled = true;
    };
  }, [waitingKey]);

  if (waiting.length === 0) return null;

  const act = async (command: "attach_profile_to_project" | "ignore_project_profile", name: string) => {
    setBusy(true);
    setError(null);
    try {
      await invoke(command, { projectName: selectedKey, profileName: name });
      await reloadProject(selectedKey);
    } catch (err) {
      const verb = command === "attach_profile_to_project" ? "attach" : "ignore";
      setError(`Could not ${verb} the profile "${name}": ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      {error && <p className="px-6 py-1.5 text-[12px] text-danger">{error}</p>}
      {waiting.map((name) => {
        const profile = profiles[name];
        const additions = profile ? describeAdditions(project, profile) : [];
        return (
          <NoticeRow
            key={name}
            severity="warning"
            title={`This repository asks for the profile "${name}".`}
            actions={
              <>
                <button onClick={() => void act("attach_profile_to_project", name)} disabled={busy} className={NOTICE_PRIMARY_BUTTON}>
                  Attach
                </button>
                <button onClick={() => void act("ignore_project_profile", name)} disabled={busy} className={NOTICE_SECONDARY_BUTTON}>
                  Ignore
                </button>
              </>
            }
          >
            {profile &&
              (additions.length > 0
                ? `Attaching it adds ${additions.join(" · ")}.`
                : "Attaching it adds nothing this project does not already have.")}
          </NoticeRow>
        );
      })}
    </div>
  );
}
