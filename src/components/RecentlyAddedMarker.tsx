/** Section header rendered at the top of a library list when recently-added
 *  items are present. */
export function RecentlyAddedSectionLabel() {
  return (
    <li className="px-3 pt-2 pb-1 select-none">
      <span className="text-[10px] font-semibold uppercase tracking-wider text-text-muted/60 flex items-center gap-1.5">
        <span className="w-1.5 h-1.5 rounded-full bg-success shrink-0" />
        Recently added
      </span>
    </li>
  );
}

/** Thin horizontal rule separating the "Recently added" group from the rest
 *  of the list. */
export function RecentlyAddedDivider() {
  return (
    <li className="px-3 mt-1 mb-0.5" aria-hidden>
      <div className="border-t border-border-strong/20" />
    </li>
  );
}
