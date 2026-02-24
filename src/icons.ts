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
   * Colour: indigo  #5E6AD2
   * Icon:   Bot
   */
  agent: {
    hex: "#5E6AD2",
    iconBox: "w-8 h-8 rounded-md bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-[#5E6AD2]",
    text: "text-[#5E6AD2]",
  },

  /**
   * Skills & prompts
   * Colour: green  #4ADE80
   * Icon:   Code
   */
  skill: {
    hex: "#4ADE80",
    iconBox: "w-8 h-8 rounded-md bg-[#4ADE80]/12 flex items-center justify-center flex-shrink-0",
    iconColor: "text-[#4ADE80]",
    text: "text-[#4ADE80]",
  },

  /**
   * MCP servers
   * Colour: amber  #F59E0B
   * Icon:   Server
   */
  mcp: {
    hex: "#F59E0B",
    iconBox: "w-8 h-8 rounded-md bg-[#F59E0B]/12 flex items-center justify-center flex-shrink-0",
    iconColor: "text-[#F59E0B]",
    text: "text-[#F59E0B]",
  },

  /**
   * Project files (CLAUDE.md, AGENTS.md, etc.)
   * Colour: indigo  #5E6AD2  (same family as agents — both are project-level)
   * Icon:   FileText
   */
  file: {
    hex: "#5E6AD2",
    iconBox: "w-8 h-8 rounded-md bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-[#5E6AD2]",
    text: "text-[#5E6AD2]",
  },

  /**
   * Projects / folders
   * Colour: indigo  #5E6AD2
   * Icon:   FolderOpen
   */
  project: {
    hex: "#5E6AD2",
    iconBox: "w-8 h-8 rounded-md bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-[#5E6AD2]",
    text: "text-[#5E6AD2]",
  },

  /**
   * Project templates
   * Colour: indigo  #5E6AD2  (default; sidebar uses dynamic accent based on contents)
   * Icon:   Layers (nav) / LayoutTemplate (detail)
   */
  projectTemplate: {
    hex: "#5E6AD2",
    iconBox: "w-8 h-8 rounded-md bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-[#5E6AD2]",
    text: "text-[#5E6AD2]",
  },

  /**
   * File templates (CLAUDE.md starters)
   * Colour: purple-ish  #8B5CF6  — distinct from project templates
   * Icon:   LayoutTemplate
   */
  fileTemplate: {
    hex: "#8B5CF6",
    iconBox: "w-8 h-8 rounded-md bg-[#8B5CF6]/15 flex items-center justify-center flex-shrink-0",
    iconColor: "text-[#8B5CF6]",
    text: "text-[#8B5CF6]",
  },
} as const;
