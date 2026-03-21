# Sub-Agents Feature

**Date:** 2026-03-14  
**Status:** Implemented

---

## Overview

Sub-agents are specialised AI assistants defined as Markdown (or TOML) files with YAML frontmatter. Each agent runs in its own context window with a custom system prompt, specific tool access, and independent permissions. Multiple coding tools discover these files from well-known directories inside a project.

This feature adds three interconnected capabilities to Automatic:

1. **Workspace Sub-Agents view** — a global registry at `~/.automatic/agents/` for creating, editing, and managing user-defined sub-agent `.md` files.
2. **Project Agents tab** — a per-project editor for project-local agents stored inline in the project JSON (written to the agent's directory on sync).
3. **Agents Marketplace** — a curated browser of community-contributed agents that can be installed into the workspace registry.

**Key distinction:** This is entirely separate from "Providers" (`Agents.tsx`). Providers = coding tools that Automatic syncs configuration to (Claude Code, Cursor, Codex, etc.). Sub-Agents = user-defined instruction files that agents execute as specialised sub-tasks.

---

## Supported Providers

The following providers support sub-agents (implement `agents_dir()`):

| Provider | Project Path | Global Path | Format |
|----------|-------------|-------------|--------|
| Claude Code | `.claude/agents/` | `~/.claude/agents/` | MD + YAML |
| Codex CLI | `.codex/agents/` | `~/.codex/agents/` | TOML (converted from MD+YAML) |
| Cursor | `.cursor/agents/` | `~/.cursor/agents/` | MD + YAML |
| Gemini CLI | `.gemini/agents/` | `~/.gemini/agents/` | MD + YAML |
| OpenCode | `.opencode/agents/` | `~/.config/opencode/agents/` | MD + YAML / JSON |

**Format conversion:** Automatic stores all workspace agents in canonical Markdown+YAML format (`~/.automatic/agents/*.md`). When syncing to Codex projects, agents are automatically converted to TOML format using the `Agent::convert_agent_content()` method. Other providers receive the canonical format directly.

Other providers (Cline, Warp, Kiro, Junie, Goose, etc.) return `None` from `agents_dir()` and are silently skipped during agent sync.

---

### Reference Format

Agents are standard Markdown files with YAML frontmatter. Example from the Claude Code documentation:

```markdown
---
name: code-reviewer
description: Expert code review specialist. Use immediately after writing or modifying code.
tools: Read, Grep, Glob, Bash
model: inherit
---

You are a senior code reviewer ensuring high standards of code quality and security.

When invoked:
1. Run git diff to see recent changes
2. Focus on modified files
3. Begin review immediately
```

Supported frontmatter fields: `name`, `description`, `tools`, `disallowedTools`, `model`, `permissionMode`, `maxTurns`, `skills`, `mcpServers`, `hooks`, `memory`, `background`, `isolation`.

---

## Architecture

### Storage

| Store | Path | Purpose |
|-------|------|---------|
| Workspace agents | `~/.automatic/agents/{machine_name}.md` | Global registry, available across all projects |
| Project-local agents | Inline in project JSON as `custom_agents` | Per-project agents, not shared |
| Synced output | `<project_dir>/.{provider}/agents/{name}.md` | Written during project sync (provider-specific) |

**Provider output directories:**
- Claude Code: `.claude/agents/`
- Codex CLI: `.codex/agents/`
- Cursor: `.cursor/agents/`
- Gemini CLI: `.gemini/agents/`
- OpenCode: `.opencode/agents/`

The workspace registry mirrors how rules work (`~/.automatic/rules/{name}.json`). Each file's raw content is the complete `.md` file (frontmatter + body). A human display name is derived from the frontmatter `name` field.

### Agent Trait Extension

The `Agent` trait in `src-tauri/src/agent/mod.rs` has an optional method:

```rust
/// Return the directory where this agent looks for sub-agent definitions.
/// Returns `None` if this agent does not support sub-agents.
fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
    None
}
```

**Providers implementing `agents_dir()`:**
- `ClaudeCode` → `.claude/agents/`
- `CodexCli` → `.codex/agents/`
- `Cursor` → `.cursor/agents/`
- `GeminiCli` → `.gemini/agents/`
- `OpenCode` → `.opencode/agents/`

During sync, if `agents_dir()` returns `None` for a provider, that provider is silently skipped for agent sync.

### Project Struct Extension

```rust
// src-tauri/src/core/projects.rs (added to Project struct)
pub custom_agents: Option<Vec<UserAgent>>,
```

Where `UserAgent` is:

```rust
pub struct UserAgent {
    pub name: String,     // human display name
    pub content: String,  // full .md content (frontmatter + body)
}
```

This mirrors `custom_rules: Option<Vec<CustomRule>>` exactly — project-local agents are embedded in the project JSON, not referenced by ID.

### Sync Pipeline

In the sync pipeline, after the existing steps (MCP config, skills, instructions), a new step:

```
For each provider in project.agents:
    if provider.agents_dir(dir).is_some():
        write each custom_agent.content to agents_dir/{machine_name}.md
        remove stale .md files not in custom_agents
```

Drift detection likewise checks the expected vs actual state of `.claude/agents/` (or equivalent).

---

## Backend Changes

### New: `src-tauri/src/core/user_agents.rs`

Mirrors `core/rules.rs`. Functions:

```rust
pub fn get_user_agents_dir() -> Result<PathBuf, String>
    // → ~/.automatic/agents/

pub fn list_user_agents() -> Result<Vec<UserAgentEntry>, String>
    // → [{id: "code-reviewer", name: "Code Reviewer"}, ...]

pub fn read_user_agent(machine_name: &str) -> Result<String, String>
    // → JSON: {"name": "Code Reviewer", "content": "---\nname: code-reviewer\n..."}

pub fn save_user_agent(machine_name: &str, name: &str, content: &str) -> Result<(), String>
    // Writes ~/.automatic/agents/{machine_name}.md

pub fn delete_user_agent(machine_name: &str) -> Result<(), String>

pub fn install_default_user_agents() -> Result<(), String>
    // Seeds bundled agents from src-tauri/agents/automatic/*.md on first run
    // Only writes if the file does not yet exist (preserves user edits)

pub fn is_valid_agent_machine_name(name: &str) -> bool
    // Lowercase slug: [a-z0-9-]
```

`UserAgentEntry`:
```rust
pub struct UserAgentEntry { pub id: String, pub name: String }
```

### New: `src-tauri/src/commands/user_agents.rs`

```rust
#[tauri::command] pub fn get_user_agents() -> Result<String, String>
#[tauri::command] pub fn read_user_agent(machine_name: String) -> Result<String, String>
#[tauri::command] pub fn save_user_agent(machine_name: String, name: String, content: String) -> Result<(), String>
#[tauri::command] pub fn delete_user_agent(machine_name: String) -> Result<(), String>
#[tauri::command] pub fn get_projects_referencing_user_agent(agent_name: String) -> Result<String, String>
    // Returns JSON array of {name, directory} for projects containing this agent in custom_agents
```

### New: Bundled Default Agents

Directory: `src-tauri/agents/automatic/` (new directory)

Files (installed to `~/.automatic/agents/` on first run, never overwritten):

- `code-reviewer.md` — Read-only code review agent (tools: Read, Grep, Glob, Bash; model: inherit)
- `debugger.md` — Debugging specialist with fix capability (tools: Read, Edit, Bash, Grep, Glob)
- `planner.md` — Architecture and planning agent (tools: Read, Grep, Glob; permissionMode: plan)

Bundled agents are embedded via `include_str!` in `user_agents.rs` and identified by the `automatic-` prefix (same convention as bundled rules). They are read-only in the UI — users must duplicate them to edit.

### Modified: `src-tauri/src/core/mod.rs`

```rust
pub mod user_agents;
pub use user_agents::*;
```

### Modified: `src-tauri/src/commands/mod.rs`

```rust
pub mod user_agents;
pub use user_agents::*;
```

### Modified: `src-tauri/src/lib.rs`

Add to `generate_handler![]`:
```rust
get_user_agents,
read_user_agent,
save_user_agent,
delete_user_agent,
get_projects_referencing_user_agent,
```

Call `core::install_default_user_agents()` in the app setup block alongside `install_default_rules()`.

### Modified: `src-tauri/src/agent/claude_code.rs`

```rust
fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
    Some(dir.join(".claude").join("agents"))
}
```

### Modified: `src-tauri/src/core/projects.rs`

Add `custom_agents: Option<Vec<UserAgent>>` to the `Project` struct with `#[serde(default)]`.

### Modified: Sync pipeline

Add agent sync step in the appropriate sync module. Details follow the sync module's existing pattern (write → cleanup stale files).

---

## Frontend Changes

### New: `src/UserAgents.tsx`

**Layout:** Two-pane, identical structure to `Rules.tsx`.

**Left sidebar:**
- Section header: "Sub-Agents"
- List of agents from `get_user_agents()`
- "+ New Agent" button at bottom
- Selection persisted to `localStorage("automatic.user-agents.selected")`

**Right pane — view/select mode:**
- Agent display name in header
- "Duplicate" button for bundled (`automatic-*`) agents
- "Delete" button (with confirmation) for user agents
- Structured form fields:
  - **Name** (display name, derived from frontmatter `name` field)
  - **Machine name** (slug, shown read-only after creation)
  - **Description** (from `description:` frontmatter field)
  - **Model** (`inherit` / `sonnet` / `haiku` / `opus` — dropdown)
  - **Tools** (multi-select chips: Read, Write, Edit, Bash, Grep, Glob, WebFetch, Agent)
  - **Permission mode** (dropdown: default / acceptEdits / dontAsk / bypassPermissions / plan)
  - **System prompt** (large textarea — the Markdown body after frontmatter)
- **"View / Edit raw"** toggle — switches to a single full-file textarea showing the complete `.md` content. Changes sync back to form fields on toggle.
- Save button (disabled when no changes)
- "Used in N projects" read-only indicator

**Right pane — new agent mode:**
- Machine name input (slug validation)
- Display name input
- Pre-populated with a minimal template

**Tauri calls:**
- `invoke("get_user_agents")` → `UserAgentEntry[]`
- `invoke("read_user_agent", { machineName })` → parsed `{ name, content }`
- `invoke("save_user_agent", { machineName, name, content })`
- `invoke("delete_user_agent", { machineName })`
- `invoke("get_projects_referencing_user_agent", { agentName })` → `{ name, directory }[]`

### New: `src/AgentsMarketplace.tsx`

Mirrors `SkillStore.tsx`. Displays cards from `featured-agents.json`.

**Layout:** Search bar at top, grid of agent cards. Clicking a card opens a detail panel.

**Card fields displayed:**
- Agent name + description (truncated)
- Model badge (sonnet / haiku / opus / inherit)
- Tools list (compact chips)
- Source link (GitHub icon)
- Install button

**Install flow:**
1. Fetch raw `.md` content from `raw_url`
2. Parse frontmatter to extract `name` as display name
3. Derive machine name from `name` field (slugified)
4. Call `save_user_agent` with the fetched content
5. Show "Installed" state on the card

Already-installed agents show "Installed" badge with an "Update" option.

### New: `src/featured-agents.json`

Hand-curated list of 10–15 community agents. Structure:

```json
[
  {
    "id": "code-reviewer",
    "name": "Code Reviewer",
    "description": "Expert code review specialist. Reviews for quality, security, and maintainability.",
    "model": "inherit",
    "tools": ["Read", "Grep", "Glob", "Bash"],
    "author": "anthropic",
    "author_url": "https://github.com/anthropic",
    "source_url": "https://github.com/anthropic/claude-code-examples",
    "raw_url": "https://raw.githubusercontent.com/..."
  }
]
```

Initial curated sources:
- Examples from the Claude Code sub-agents documentation (code-reviewer, debugger, data-scientist, db-reader)
- Selected agents from `wshobson/agents` (31k stars) — ui-visual-validator, accessibility agents, etc.

### Modified: `src/App.tsx`

**Imports:**
```tsx
import UserAgents from "./UserAgents";
import AgentsMarketplace from "./AgentsMarketplace";
```

**Sidebar — Workspace group** (after the Rules `NavItem`):
```tsx
<NavItem id="user-agents" icon={Bot} label="Sub-Agents" />
```

**Sidebar — Marketplace group** (after existing marketplace items):
```tsx
<NavItem id="agents-marketplace" icon={Store} label="Agents" />
```

**Conditional renders:**
```tsx
{activeTab === "user-agents" && <UserAgents />}
{activeTab === "agents-marketplace" && <AgentsMarketplace />}
```

### Modified: `src/Projects.tsx`

**Tab type extension:**
```typescript
type ProjectTab = "summary" | "agents" | "custom_agents" | "skills" | ...
```

**`configuration` group gains a new tab:**
```typescript
{ id: "configuration", label: "Configuration", tabs: [
    { id: "agents",        label: "Providers" },
    { id: "custom_agents", label: "Agents" },   // new
    { id: "skills",        label: "Skills" },
    { id: "mcp_servers",   label: "MCP Servers" },
]},
```

**New tab content (`projectTab === "custom_agents"`):**

A mini-CRUD editor, structurally similar to the Custom Rules section:

- List of `project.custom_agents` with name chips and an edit button per row
- "Add Agent" button → inline form (name + content editor)
- Edit mode: name field + structured form + raw toggle
- Delete with inline confirmation
- On any change: `setDirty(true)`
- Informational note: _"These agents are written to `.claude/agents/` (and other supported providers) when the project is synced."_
- If any provider in `project.agents` does not support `agents_dir()`, show a muted warning: _"[Provider] does not yet support agent sync"_

---

## File Change Summary

### New Files

| Path | Description |
|------|-------------|
| `src-tauri/src/core/user_agents.rs` | CRUD business logic for `~/.automatic/agents/` |
| `src-tauri/src/commands/user_agents.rs` | Tauri command wrappers |
| `src-tauri/agents/automatic/code-reviewer.md` | Bundled default agent |
| `src-tauri/agents/automatic/debugger.md` | Bundled default agent |
| `src-tauri/agents/automatic/planner.md` | Bundled default agent |
| `src/UserAgents.tsx` | Workspace Sub-Agents CRUD view |
| `src/AgentsMarketplace.tsx` | Agents marketplace browser |
| `src/featured-agents.json` | Curated featured agents list |
| `docs/sub-agents.md` | This document |

### Modified Files

| Path | Change |
|------|--------|
| `src-tauri/src/core/mod.rs` | `pub mod user_agents; pub use user_agents::*;` |
| `src-tauri/src/commands/mod.rs` | `pub mod user_agents; pub use user_agents::*;` |
| `src-tauri/src/lib.rs` | Register 5 new commands; call `install_default_user_agents()` |
| `src-tauri/src/agent/mod.rs` | Add `agents_dir()` method to `Agent` trait (default `None`) |
| `src-tauri/src/agent/claude_code.rs` | Implement `agents_dir()` → `.claude/agents/` |
| `src-tauri/src/core/projects.rs` | Add `custom_agents: Option<Vec<UserAgent>>` to `Project` |
| `src-tauri/src/sync/` | Add agent file sync step in project sync pipeline |
| `src/App.tsx` | 2 imports, 2 `NavItem`s, 2 conditional renders |
| `src/Projects.tsx` | New `custom_agents` tab in Configuration group |

---

## Implementation Order

1. Backend core — `core/user_agents.rs` + bundled `.md` files
2. Backend commands — `commands/user_agents.rs` + registration in `lib.rs`
3. Agent trait — `agents_dir()` method + `ClaudeCode` implementation
4. Project struct — `custom_agents` field
5. Sync pipeline — agent file write/cleanup step
6. `src/UserAgents.tsx` — workspace CRUD view
7. `src/App.tsx` — nav wiring
8. `src/Projects.tsx` — project Agents tab
9. `src/featured-agents.json` + `src/AgentsMarketplace.tsx` — marketplace

Each step is independently verifiable: backend steps via `cargo check` + `cargo test`; frontend steps via `npm run build`.

---

## Open Questions

- **Drift detection scope:** Should the presence/absence of agent files in `.claude/agents/` be included in the existing drift check? This would make drift detection flag projects where agents have been manually edited or deleted outside Automatic.
- **Codex support:** Codex uses `AGENTS.md` for project instructions but its sub-agent format (if it has one) is not yet documented. The `agents_dir()` default of `None` is safe for now.
- **Raw URL reliability:** For the marketplace, fetching directly from GitHub raw URLs may be rate-limited or unavailable offline. A fallback (bundled content for featured agents) should be considered.
