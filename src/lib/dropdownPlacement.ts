/** Which side of its trigger a dropdown menu opens on. */
export type DropdownSide = "below" | "above";

/** Gap between the trigger and the menu, matching the `mt-1` / `mb-1` classes. */
const MENU_GAP_PX = 4;

interface VerticalSpan {
  top: number;
  bottom: number;
}

/**
 * Decides whether a dropdown should open below its trigger or flip above it.
 *
 * `bounds` is the visible area the menu must fit in: the nearest clipping or
 * scrolling ancestor, limited to the viewport. The menu stays below unless it
 * does not fit there and there is more room above.
 */
export function chooseDropdownSide(
  anchor: VerticalSpan,
  bounds: VerticalSpan,
  menuHeight: number,
): DropdownSide {
  const needed = menuHeight + MENU_GAP_PX;
  const spaceBelow = bounds.bottom - anchor.bottom;
  const spaceAbove = anchor.top - bounds.top;
  return spaceBelow < needed && spaceAbove > spaceBelow ? "above" : "below";
}

const CLIPPING_OVERFLOW = new Set(["auto", "scroll", "hidden", "clip"]);

/**
 * Returns the vertical extent a menu anchored to `element` can occupy before
 * an ancestor clips it or pushes it behind a scroll: the nearest ancestor with
 * a clipping `overflow-y`, intersected with the viewport.
 */
export function visibleBoundsFor(element: HTMLElement): VerticalSpan {
  let top = 0;
  let bottom = window.innerHeight;
  for (let node = element.parentElement; node; node = node.parentElement) {
    if (CLIPPING_OVERFLOW.has(getComputedStyle(node).overflowY)) {
      const rect = node.getBoundingClientRect();
      top = Math.max(top, rect.top);
      bottom = Math.min(bottom, rect.bottom);
      break;
    }
  }
  return { top, bottom };
}
