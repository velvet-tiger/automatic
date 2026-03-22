/**
 * TechMeshBackground
 *
 * Full-area decorative background: three slow-drifting radial gradient orbs
 * layered over a shimmering dot grid and an animated network-graph line grid.
 *
 * Usage
 * -----
 * Drop as the first child of any `relative`-positioned container.
 * The parent must establish a positioning context:
 *
 *   <div className="relative flex-1 h-full">
 *     <TechMeshBackground />
 *     <YourContent />   // add z-10 or higher if needed
 *   </div>
 *
 * The component is purely decorative (aria-hidden, pointer-events: none).
 *
 * Props
 * -----
 * id?: string   — suffix appended to SVG pattern IDs to avoid collisions
 *                 when multiple instances appear on the same page.
 */

interface TechMeshBackgroundProps {
  id?: string;
}

export default function TechMeshBackground({ id = "" }: TechMeshBackgroundProps) {
  const dotId = `dot-pattern${id}`;
  const gridId = `grid-pattern${id}`;

  return (
    <div className="dashboard-bg" aria-hidden="true">
      {/* Orb 1 — indigo, top-left, 22 s drift */}
      <div className="dashboard-bg__orb-1" />
      {/* Orb 2 — cyan, bottom-right, 28 s drift */}
      <div className="dashboard-bg__orb-2" />
      {/* Orb 3 — violet, centre-right, 18 s drift */}
      <div className="dashboard-bg__orb-3" />

      {/* Dot grid — 28×28 px tile, 6 s shimmer */}
      <svg className="dashboard-bg__dots" xmlns="http://www.w3.org/2000/svg">
        <defs>
          <pattern id={dotId} x="0" y="0" width="28" height="28" patternUnits="userSpaceOnUse">
            <circle cx="1.5" cy="1.5" r="1.5" fill="rgba(148,155,210,0.35)" />
          </pattern>
        </defs>
        <rect width="100%" height="100%" fill={`url(#${dotId})`} />
      </svg>

      {/* Network graph grid — 112×112 px tile, 8 s pulse
          cardinal lines + diagonal connectors + node circles */}
      <svg className="dashboard-bg__grid" xmlns="http://www.w3.org/2000/svg">
        <defs>
          <pattern id={gridId} x="0" y="0" width="112" height="112" patternUnits="userSpaceOnUse">
            <line x1="0"   y1="56"  x2="112" y2="56"  stroke="rgba(94,106,210,0.25)" strokeWidth="0.75" />
            <line x1="56"  y1="0"   x2="56"  y2="112" stroke="rgba(94,106,210,0.25)" strokeWidth="0.75" />
            <line x1="0"   y1="0"   x2="56"  y2="56"  stroke="rgba(34,211,238,0.10)" strokeWidth="0.5" />
            <line x1="112" y1="0"   x2="56"  y2="56"  stroke="rgba(34,211,238,0.10)" strokeWidth="0.5" />
            <circle cx="0"   cy="0"   r="2"   fill="rgba(94,106,210,0.30)" />
            <circle cx="112" cy="0"   r="2"   fill="rgba(94,106,210,0.30)" />
            <circle cx="0"   cy="112" r="2"   fill="rgba(94,106,210,0.30)" />
            <circle cx="112" cy="112" r="2"   fill="rgba(94,106,210,0.30)" />
            <circle cx="56"  cy="0"   r="1.5" fill="rgba(94,106,210,0.20)" />
            <circle cx="0"   cy="56"  r="1.5" fill="rgba(94,106,210,0.20)" />
            <circle cx="112" cy="56"  r="1.5" fill="rgba(94,106,210,0.20)" />
            <circle cx="56"  cy="112" r="1.5" fill="rgba(94,106,210,0.20)" />
            <circle cx="56"  cy="56"  r="2.5" fill="rgba(139,92,246,0.28)" />
          </pattern>
        </defs>
        <rect width="100%" height="100%" fill={`url(#${gridId})`} />
      </svg>
    </div>
  );
}
