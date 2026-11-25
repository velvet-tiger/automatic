import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { chooseDropdownSide, visibleBoundsFor, type DropdownSide } from "../lib/dropdownPlacement";

interface DropdownMenuProps {
  /**
   * Positioning and styling classes. Vertical placement classes (`top-*`,
   * `bottom-*`, `mt-*`, `mb-*`) are removed: this component adds them for
   * whichever side the menu opens on.
   */
  className: string;
  children: ReactNode;
}

// Matches Tailwind vertical placement utilities, with optional variant prefixes
// (`md:top-full`) and negative values (`-mt-1`).
const VERTICAL_PLACEMENT_CLASS = /^(?:[\w-]+:)*-?(?:top|bottom|mt|mb)-/;

/**
 * Drops vertical placement classes from a caller's class list. A stray
 * `top-full` alongside the `bottom-full` added on flip gives the menu both
 * edges, which collapses it to zero height (VEL-456).
 */
export function stripVerticalPlacement(className: string): string {
  return className
    .split(/\s+/)
    .filter((cls) => cls !== "" && !VERTICAL_PLACEMENT_CLASS.test(cls))
    .join(" ");
}

/**
 * A menu anchored to its `relative` parent that opens below the parent by
 * default and flips above it when it would not fit in the visible area.
 *
 * Mount it only while the menu is open. The side is chosen once on mount, so
 * filtering a long list does not make the menu jump between sides.
 */
export function DropdownMenu({ className, children }: DropdownMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [side, setSide] = useState<DropdownSide>("below");

  useLayoutEffect(() => {
    const menu = ref.current;
    const anchor = menu?.parentElement;
    if (!menu || !anchor) return;
    setSide(
      chooseDropdownSide(anchor.getBoundingClientRect(), visibleBoundsFor(anchor), menu.offsetHeight),
    );
  }, []);

  const sideClasses = side === "above" ? "bottom-full mb-1" : "top-full mt-1";
  return (
    <div ref={ref} className={`${stripVerticalPlacement(className)} ${sideClasses}`}>
      {children}
    </div>
  );
}
