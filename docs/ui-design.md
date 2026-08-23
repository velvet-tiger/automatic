# Workspace UI Design Rules

Rules for the Automatic desktop Workspace: project overview, sidebar, and project editor. They come from the 2026-08 quieting pass (Cursor-inspired hierarchy, less chrome). Follow them when changing those surfaces.

Use existing tokens from `src/App.css` / Tailwind theme (`bg-bg-base`, `bg-bg-input`, `bg-bg-sidebar`, `border-border-strong`, `text-text-base`, `text-text-muted`, `brand`, health vars). Do not invent a new palette.

## Principles

1. **Structure over decoration.** Order content by importance. Do not use color, gradients, or badges to invent hierarchy.
2. **Quiet by default.** Normal states are muted. Color is for exception (drifted, missing, danger), not for “synced” or “healthy.”
3. **Attention first, inventory second.** Gaps and actions that need a decision sit above counts and catalogs.
4. **One job per strip.** A toolbar, title block, or metric line should not also be a dashboard.
5. **Match density across Workspace.** Overview cards, Summary, and the project header should feel like one product.

## Borders and surfaces

- Prefer `border-border-strong/35` (or `/40` at most) on cards and panels.
- Avoid wrapping a whole card in warning/success border for status. Put status in text.
- Avoid brand-tinted icon wells (`bg-icon-*/10`, `bg-brand/10`) on inventory tiles. Use a single muted icon, or no well.
- Avoid gradient callouts (`from-brand/10 to-brand/5`). Use a quiet card: `bg-bg-input` + thin border.

## Status and badges

- **Synced / healthy:** muted text (`text-text-muted/45`–`/50`). No success pills.
- **Drifted / missing / incomplete:** warning or danger text. Still prefer text over filled pills when space allows.
- **Recommendations / counts:** muted tabular numbers or a text link (`N recommendations` → `Review`). Not a warning banner or rounded-full badge.
- Drop Set/Missing style pills on utility widgets. Say the state in plain muted copy.

## Metrics and inventory

- Do not present a grid of large tinted metric cards as the primary Summary content.
- Prefer one muted inventory line of clickable segments:  
  `N skills · N mcp · N rules · N agents · N cmds · N hooks`  
  Style: `text-[12px] text-text-muted/50`, stronger on hover. Each segment opens its tab.
- On overview project cards, secondary metrics (skills · mcp · …) appear on hover or focus, not always on.

## Agents and groups

- Agent identity is **icon-only** (`AgentIcon`), not bordered name chips.
- Place agent icons on the **trailing edge** of the title row (right side), size about **14px**.
- Align leading icons with the **top of the title text**, not the vertical middle of title + subtitle (`items-start`).
- Group membership is **muted text links**, not `rounded-full` pills.
- In lists that show everything by group: **named groups first** (A–Z), then **Other Projects** (ungrouped).

## Toolbars and controls

- Shared toolbar control height: **`h-7`** (search, sort, toggles, primary actions in the same row).
- Primary action (e.g. Add Project) uses brand fill; secondary actions (Sync all) stay outline / transparent.
- Segmented view toggles (grid / table) use a quiet bordered group; active segment is `bg-brand/15 text-brand`, not a heavy fill.

## Project header and tabs

- Project title: about **16px** semibold, tight padding (`pt-4 pb-3`), not a large hero.
- Path under the title stays mono, `text-[11px]`, muted.
- Primary tabs: about **12px**, quieter inactive color (`text-text-muted/70`). Active indicator is a **thin** brand line (`h-px`, inset), not a thick full-width bar.
- Secondary sub-tabs follow the same language, slightly softer.

## Sidebar (Workspace)

- Top action is **View All** (clears group filter, opens full projects list). Do not put Add Project in the sidebar; keep create on the overview toolbar.
- Section label **Projects** + create-group control.
- **Groups** use chevron expand/collapse; count is muted tabular text.
- **Other Projects** follows groups.
- **Groups** management link sits at the bottom with a separator (`mt-auto` + top border) when space allows.

## Project Summary layout

Order content top to bottom:

1. Attention (incomplete setup checklist and/or recommendations link)
2. Muted inventory line
3. Recent activity (primary body)
4. Utility column (Instructions, Docs, Memory) without hero numbers or status pills

Complete Setup keeps the checklist. It loses gradient, tinted package wells, and brand-numbered circles. Use quiet numbered text steps.

## Reference surfaces

| Surface | Primary files |
|---|---|
| Overview cards / Show Groups | `src/pages/workspace/projects/overview/ProjectsOverview.tsx` |
| Overview mockup (Before/After) | `src/pages/workspace/projects/overview/workspace-overview-mockup.html` |
| Workspace sidebar | `src/components/WorkspaceSidebar.tsx` |
| Project Summary | `src/pages/workspace/projects/editor/panels/SummaryPanel.tsx`, `SummaryMetricCard.tsx` |
| Project header / tabs | `src/pages/workspace/projects/editor/ProjectEditor.tsx` |

## Out of scope for these rules

- Library, Discover, Settings, and marketing/web surfaces (unless you intentionally extend the language there)
- Token or theme redesign
- Changing tab *structure* (`PROJECT_GROUPS` / `PROJECT_CONTROLS`); these rules cover chrome and density only
