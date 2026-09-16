import { Layers, Lock } from "lucide-react";

/** Pill shown in a drawer header for a built-in item that ships with Automatic. */
export function BuiltInBadge({ label = "Built-in" }: { label?: string }) {
  return (
    <span className="text-[10px] font-semibold text-text-muted tracking-wider uppercase px-2 py-1 rounded-full bg-brand/10 border border-brand/20">
      {label}
    </span>
  );
}

/** Pill shown in a drawer header for an item that cannot currently be edited (e.g. installed from a remote source). */
export function ReadOnlyBadge({ label = "Read-only", tooltip }: { label?: string; tooltip?: string }) {
  return (
    <span
      className="flex items-center gap-1 px-2 py-1 rounded text-[11px] text-text-muted bg-bg-sidebar border border-border-strong/40"
      title={tooltip}
    >
      <Lock size={10} />
      <span>{label}</span>
    </span>
  );
}

/** Lock icon shown in a table row's checkbox cell in place of a checkbox, for a row that cannot be selected or deleted. */
export function LockCell({ tooltip }: { tooltip: string }) {
  return (
    <span title={tooltip}>
      <Lock size={11} className="text-text-muted/60" />
    </span>
  );
}

/**
 * In-row pill for an entry a profile provides to a project. The profile owns
 * the entry, so the row that renders this badge hides its remove control.
 */
export function InheritedBadge({ profile }: { profile: string }) {
  return (
    <span
      className="inline-flex items-center gap-1 text-[10px] text-text-muted bg-bg-sidebar border border-border-strong/40 rounded px-1.5 py-0.5 whitespace-nowrap flex-shrink-0"
      title={`Provided by profile ${profile}. Edit the profile to change it.`}
    >
      <Layers size={10} className="shrink-0" />
      <span>Profile: {profile}</span>
    </span>
  );
}
