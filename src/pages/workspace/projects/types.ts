// Shared domain types for the Projects workspace feature.
//
// These were extracted verbatim from Projects.tsx as part of a behavior-preserving
// refactor. Component prop interfaces (`*Props`) stay co-located with their
// components; only cross-cutting domain types live here.

import type { AgentOptions } from "../../../components/AgentSelector";

export interface CustomRule {
  name: string;
  content: string;
}

export interface CustomAgent {
  name: string;
  content: string;
}

export interface CustomCommand {
  name: string;
  content: string;
}

export interface CustomSkill {
  name: string;
  content: string;
}

export interface SubagentEntry {
  id: string;
  name: string;
}

export interface UserCommandEntry {
  id: string;
  description: string;
}

export interface Project {
  name: string;
  description: string;
  directory: string;
  skills: string[];
  mcp_servers: string[];
  disabled_mcp_servers?: string[];
  providers: string[];
  agents: string[];
  created_at: string;
  updated_at: string;
  last_activity?: string;
  created_by?: string;
  file_rules?: Record<string, string[]>;
  instruction_mode?: string;
  /** Per-agent options keyed by agent id. Agents not present use defaults. */
  agent_options?: Record<string, AgentOptions>;
  /** Inline custom rules stored directly in this project (not in the global registry). */
  custom_rules?: CustomRule[];
  /** Inline custom agents stored directly in this project. Written to .claude/agents/ (or equivalent) on sync. */
  custom_agents?: CustomAgent[];
  /** Tool names detected as present in this project (populated by autodetect). */
  tools?: string[];
  /** Workspace agent names selected for this project. Written to agent's sub-agent directory on sync. */
  user_agents?: string[];
  /** Workspace command names selected for this project. Written to .agents/commands and then synced into provider command directories. */
  user_commands?: string[];
  /** Hook machine names selected for this project. Each hook's target agent is in its library record; the sync engine groups by agent. */
  hooks?: string[];
  /** Inline custom commands stored directly in this project. */
  custom_commands?: CustomCommand[];
  /** Inline custom skills stored directly in this project. Written to skill directories on sync. */
  custom_skills?: CustomSkill[];
  /** When true, rules are written to .automatic/instructions/ and the instruction file becomes an index. */
  instructions_index_mode?: boolean;
  /**
   * When true, Automatic maintains a managed block in the project's .gitignore
   * listing every path it writes, so generated agent config is not committed.
   * The block is removed on the next sync when this is turned back off.
   */
  manage_gitignore?: boolean;
  /**
   * Sync mode for this project.
   * - "normal" (default): files are written directly into the project directory.
   * - "silent": files that would normally go outside .automatic/ are redirected
   *   to .automatic/silent/, leaving the rest of the project tree untouched.
   */
  mode?: 'normal' | 'silent';
  /** Computed by backend at read-time: true when directory is set but no longer exists on disk. */
  directory_missing?: boolean;
}

export interface AgentCapabilities {
  skills: boolean;
  instructions: boolean;
  mcp_servers: boolean;
  agents: boolean;
  commands: boolean;
  hooks: boolean;
}

export interface AgentInfo {
  id: string;
  label: string;
  description: string;
  /** Non-null when this agent cannot have MCP config written by Automatic. */
  mcp_note: string | null;
  capabilities?: AgentCapabilities;
}

export interface HookEntry {
  id: string;
  name: string;
  agent: string;
  event: string;
  plugin_id?: string | null;
}

export interface DriftedFile {
  path: string;
  reason: "missing" | "modified" | "stale" | "unreadable";
  /** Content Automatic would generate. Present only when reason === "modified". */
  expected?: string;
  /** Content currently on disk. Present only when reason === "modified". */
  actual?: string;
}

export interface AgentDrift {
  agent_id: string;
  agent_label: string;
  files: DriftedFile[];
}

export interface InstructionFileConflict {
  /** The instruction filename (e.g. "AGENTS.md", "CLAUDE.md"). */
  filename: string;
  /** Agent labels that use this file. */
  agent_labels: string[];
  /** User-authored content currently on disk (managed sections stripped). */
  disk_content: string;
  /** User-authored content Automatic has stored (empty if never set through Automatic). */
  automatic_content: string;
}

export interface CustomSkillConflict {
  /** Machine-name of the custom skill. */
  skill_name: string;
  /** Relative path of the on-disk SKILL.md. */
  path: string;
  /** Content currently on disk. */
  disk_content: string;
  /** Content stored in the project's custom_skills entry. */
  automatic_content: string;
}

export type CustomAssetKind = "skill" | "rule" | "agent" | "command";

export interface CustomAssetConflict {
  kind: CustomAssetKind;
  name: string;
  path: string;
  disk_content: string;
  automatic_content: string;
}

export interface UnifiedCandidate {
  filename: string;
  agent_labels: string[];
  user_content: string;
  exists: boolean;
  modified_ms: number | null;
}

export interface UnifiedInspection {
  candidates: UnifiedCandidate[];
  consistent: boolean;
}

export interface RebuildPreviewCategory {
  key: string;
  label: string;
  automatic: string[];
  disk: string[];
  added: string[];
  removed: string[];
}

export interface RebuildPreview {
  project_name: string;
  categories: RebuildPreviewCategory[];
  changed: boolean;
}

export interface DriftReport {
  drifted: boolean;
  agents: AgentDrift[];
  /** Instruction files that have external content Automatic does not recognise. */
  instruction_conflicts?: InstructionFileConflict[];
  /** Project-scoped custom assets whose on-disk files differ from stored content. */
  custom_conflicts?: CustomAssetConflict[];
}

export type ProjectProblemKind = "mcp_user_scope_conflict";

export interface ProjectProblem {
  kind: ProjectProblemKind;
  title: string;
  description: string;
  reference_url?: string;
  agents: string[];
  resources: string[];
}

export interface ProjectProblemsReport {
  has_problems: boolean;
  problems: ProjectProblem[];
}

export interface ProjectFileInfo {
  filename: string;
  agents: string[];
  exists: boolean;
  target_files?: string[];
}

export interface TemplateProjectFile {
  filename: string;
  content: string;
}

export interface ProjectTemplate {
  name: string;
  description: string;
  skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  user_agents: string[];
  user_commands: string[];
  hooks: string[];
  project_files: TemplateProjectFile[];
  unified_instruction?: string;
  unified_rules?: string[];
}

export interface ActivityEntry {
  id: number;
  project: string;
  event: string;
  label: string;
  detail: string;
  timestamp: string;
}

export interface ProjectRecommendation {
  id: number;
  project: string;
  kind: string;
  title: string;
  body: string;
  priority: "low" | "normal" | "high";
  status: "pending" | "dismissed" | "actioned";
  source: string;
  /** Optional JSON blob with extra data, e.g. `{"id":"owner/repo/skill","name":"skill","source":"owner/repo","installs":0}` */
  metadata: string;
  created_at: string;
  updated_at: string;
}

export interface ProjectToolEntry {
  name: string;
  display_name: string;
  description: string;
  url: string;
  github_repo?: string;
  kind: "cli" | "doc_gen" | "analyser" | "planning" | "server" | "other";
  detect_binary?: string;
  detect_dir?: string;
  plugin_id?: string;
  /** `true` = binary on PATH, `false` = not found, `null` = no detect_binary */
  detected: boolean | null;
  /** When `true`, this tool contributes a top-level tab in the project nav. */
  provides_tab: boolean;
  /** When `false`, this tool has no per-project effect and should not be offered in the Project Tools tab. */
  project_scoped: boolean;
}
