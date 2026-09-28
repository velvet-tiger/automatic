import type { ProjectLabel } from "../lib/projectIdentity";

interface ProjectNameLabelProps {
  label: ProjectLabel;
  /** Classes for the outer span, e.g. the text size, weight and `truncate` the name had before. */
  className?: string;
}

/**
 * A project's name, followed by a muted folder hint when another project
 * has the same name (see `projectLabels`). A unique name renders as the
 * plain name in one span, exactly as before.
 */
export function ProjectNameLabel({ label, className }: ProjectNameLabelProps) {
  if (label.hint === null) {
    return <span className={className}>{label.name}</span>;
  }
  return (
    <span className={className} title={`${label.name} · ${label.hint}`}>
      {label.name}
      <span className="font-normal text-text-muted"> · {label.hint}</span>
    </span>
  );
}
