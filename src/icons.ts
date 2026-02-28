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
   * Colour: indigo  var(--icon-agent)
   * Icon:   Bot
   */
  agent: {
    hex: "var(--icon-agent)",
    iconBox: "w-8 h-8 rounded-md bg-icon-agent/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-icon-agent",
    text: "text-icon-agent",
  },

  /**
   * Skills & prompts
   * Colour: green  var(--icon-skill)
   * Icon:   Code
   */
  skill: {
    hex: "var(--icon-skill)",
    iconBox: "w-8 h-8 rounded-md bg-icon-skill/12 flex items-center justify-center flex-shrink-0",
    iconColor: "text-icon-skill",
    text: "text-icon-skill",
  },

  /**
   * MCP servers
   * Colour: amber  var(--icon-mcp)
   * Icon:   Server
   */
  mcp: {
    hex: "var(--icon-mcp)",
    iconBox: "w-8 h-8 rounded-md bg-icon-mcp/12 flex items-center justify-center flex-shrink-0",
    iconColor: "text-icon-mcp",
    text: "text-icon-mcp",
  },

  /**
   * Project files (CLAUDE.md, AGENTS.md, etc.)
   * Colour: indigo  var(--icon-agent)  (same family as agents — both are project-level)
   * Icon:   FileText
   */
  file: {
    hex: "var(--icon-agent)",
    iconBox: "w-8 h-8 rounded-md bg-icon-agent/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-icon-agent",
    text: "text-icon-agent",
  },

  /**
   * Projects / folders
   * Colour: indigo  var(--icon-agent)
   * Icon:   FolderOpen
   */
  project: {
    hex: "var(--icon-agent)",
    iconBox: "w-8 h-8 rounded-md bg-icon-agent/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-icon-agent",
    text: "text-icon-agent",
  },

  /**
   * Project templates
   * Colour: indigo  var(--icon-agent)  (default; sidebar uses dynamic accent based on contents)
   * Icon:   Layers (nav) / LayoutTemplate (detail)
   */
  projectTemplate: {
    hex: "var(--icon-agent)",
    iconBox: "w-8 h-8 rounded-md bg-icon-agent/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-icon-agent",
    text: "text-icon-agent",
  },

  /**
   * File templates (CLAUDE.md starters)
   * Colour: purple-ish  var(--icon-file-template)  — distinct from project templates
   * Icon:   LayoutTemplate
   */
  fileTemplate: {
    hex: "var(--icon-file-template)",
    iconBox: "w-8 h-8 rounded-md bg-icon-file-template/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-icon-file-template",
    text: "text-icon-file-template",
  },
  /**
   * Rules (reusable content blocks for project instructions)
   * Colour: cyan  var(--icon-rule)  — distinct from templates and skills
   * Icon:   ScrollText
   */
  rule: {
    hex: "var(--icon-rule)",
    iconBox: "w-8 h-8 rounded-md bg-icon-rule/12 flex items-center justify-center flex-shrink-0",
    iconColor: "text-icon-rule",
    text: "text-icon-rule",
  },
} as const;
