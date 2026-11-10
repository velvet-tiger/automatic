import type { FolderSource, WritableFoldersFindings } from "./types";

/** The last part of a folder path, which is how a folder is named on screen. */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter((part) => part !== "");
  return parts[parts.length - 1] ?? path;
}

/**
 * The folder that holds `path`, with the home folder shown as `~`. Shown
 * small under the name, so two folders with the same name can be told apart.
 */
export function folderParent(path: string, home: string | null): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  const parent = cut > 0 ? trimmed.slice(0, cut) : trimmed.slice(0, cut + 1);
  if (!home) return parent;
  const base = home.replace(/[\\/]+$/, "");
  if (parent === base) return "~";
  if (parent.startsWith(`${base}/`) || parent.startsWith(`${base}\\`)) return `~${parent.slice(base.length)}`;
  return parent;
}

/** Why an extra folder is not shared with Claude Code right now, or null when it is. */
export type ExtraFolderState =
  | { kind: "ok" }
  | { kind: "missing" }
  | { kind: "refused"; reason: string };

/**
 * The state of each stored extra folder. Missing and refused folders keep
 * the path as stored, which is how the list stores them, so they match.
 */
export function extraFolderState(path: string, findings: WritableFoldersFindings | null): ExtraFolderState {
  if (!findings) return { kind: "ok" };
  const isExtra = (source: FolderSource) => source.kind === "extra";
  if (findings.missing.some((m) => m.path === path && isExtra(m.source))) return { kind: "missing" };
  const refused = findings.refused.find((r) => r.path === path && isExtra(r.source));
  if (refused) return { kind: "refused", reason: refused.reason };
  return { kind: "ok" };
}

/** One group's line under "From your groups". */
export interface GroupFolders {
  group: string;
  /** Member project names whose folders Claude Code may edit. Empty when the switch is off. */
  projects: string[];
  switchedOn: boolean;
}

/**
 * The groups this project joined, with the members whose folders they add.
 * Groups with the switch on come first, then the ones with it off.
 */
export function groupFolders(findings: WritableFoldersFindings): GroupFolders[] {
  const on = new Map<string, Set<string>>();
  const sources = [...findings.wanted, ...findings.missing, ...findings.refused].map((f) => f.source);
  for (const source of sources) {
    if (source.kind !== "group_member") continue;
    const members = on.get(source.group) ?? new Set<string>();
    members.add(source.project);
    on.set(source.group, members);
  }
  const lines: GroupFolders[] = [...on.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([group, members]) => ({
      group,
      projects: [...members].sort((a, b) => a.localeCompare(b)),
      switchedOn: true,
    }));
  for (const group of [...findings.groups_switched_off].sort((a, b) => a.localeCompare(b))) {
    if (!on.has(group)) lines.push({ group, projects: [], switchedOn: false });
  }
  return lines;
}

/** The status line of the card: where the folders were saved, in plain words. */
export type SavedStatus = { text: string; ok: boolean } | null;

export function savedStatus(findings: WritableFoldersFindings): SavedStatus {
  if (findings.wanted.length === 0) return null;
  if (findings.last_sync_errors.length > 0) {
    return { text: "Not saved. The last sync failed (see the notices above)", ok: false };
  }
  switch (findings.target.kind) {
    case "local":
      return { text: "Saved for this project only", ok: true };
    case "user":
      return { text: "Saved for all your Claude Code sessions", ok: true };
    case "needs_consent":
      return { text: "Not saved. Waiting for your OK (see the notices above)", ok: false };
  }
}
