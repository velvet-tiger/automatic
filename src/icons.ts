/**
 * Design-system icon tokens for Nexus.
 *
 * Each concept has a canonical lucide-react icon, a hex accent colour, and
 * three ready-to-use Tailwind class bundles so every view looks identical:
 *
 *   iconBox   — the coloured square used in list rows and detail panes
 *               e.g. <div className={ICONS.skill.iconBox}><Code size={15} /></div>
 *
 *   iconColor — plain icon tint, used for section headings and inline icons
 *               e.g. <Code size={13} className={ICONS.skill.iconColor} />
 *
 *   text      — accent text, used for badges, labels and status text
 *               e.g. <span className={ICONS.skill.text}>active</span>
 *
 * Import:
 *   import { ICONS } from "./icons";
 *   import { Bot, Code, Server, FileText, LayoutTemplate, FolderOpen } from "lucide-react";
 */

export const ICONS = {
  /**
   * Agent tools (Claude Code, Cursor, etc.)
   * Colour: indigo  var(--brand)
   * Icon:   Bot
   */
  agent: {
    hex: "var(--brand)",
    iconBox: "w-8 h-8 rounded-md bg-brand/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-brand",
    text: "text-brand",
  },

  /**
   * Skills & prompts
   * Colour: green  var(--success)
   * Icon:   Code
   */
  skill: {
    hex: "var(--success)",
    iconBox: "w-8 h-8 rounded-md bg-success/12 flex items-center justify-center flex-shrink-0",
    iconColor: "text-success",
    text: "text-success",
  },

  /**
   * MCP servers
   * Colour: amber  var(--warning)
   * Icon:   Server
   */
  mcp: {
    hex: "var(--warning)",
    iconBox: "w-8 h-8 rounded-md bg-warning/12 flex items-center justify-center flex-shrink-0",
    iconColor: "text-warning",
    text: "text-warning",
  },

  /**
   * Project files (CLAUDE.md, AGENTS.md, etc.)
   * Colour: indigo  var(--brand)  (same family as agents — both are project-level)
   * Icon:   FileText
   */
  file: {
    hex: "var(--brand)",
    iconBox: "w-8 h-8 rounded-md bg-brand/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-brand",
    text: "text-brand",
  },

  /**
   * Projects / folders
   * Colour: indigo  var(--brand)
   * Icon:   FolderOpen
   */
  project: {
    hex: "var(--brand)",
    iconBox: "w-8 h-8 rounded-md bg-brand/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-brand",
    text: "text-brand",
  },

  /**
   * Project templates
   * Colour: indigo  var(--brand)  (default; sidebar uses dynamic accent based on contents)
   * Icon:   Layers (nav) / LayoutTemplate (detail)
   */
  projectTemplate: {
    hex: "var(--brand)",
    iconBox: "w-8 h-8 rounded-md bg-brand/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-brand",
    text: "text-brand",
  },

  /**
   * File templates (CLAUDE.md starters)
   * Colour: purple-ish  var(--accent)  — distinct from project templates
   * Icon:   LayoutTemplate
   */
  fileTemplate: {
    hex: "var(--accent)",
    iconBox: "w-8 h-8 rounded-md bg-accent/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-accent",
    text: "text-accent",
  },
  /**
   * Rules (reusable content blocks for project instructions)
   * Colour: cyan  var(--accent-hover)  — distinct from templates and skills
   * Icon:   ScrollText
   */
  rule: {
    hex: "var(--accent-hover)",
    iconBox: "w-8 h-8 rounded-md bg-accent-hover/12 flex items-center justify-center flex-shrink-0",
    iconColor: "text-accent-hover",
    text: "text-accent-hover",
  },
} as const;
