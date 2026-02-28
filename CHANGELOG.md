# Changelog

All notable changes to Automatic are documented here.

## [0.2.0] — 2026-02-28

### Features

#### Shell & UI
- Custom overlay titlebar with Linear-style header layout replacing the default macOS titlebar
- Sidebar logo text increased in size and rendered in pure white
- Add buttons restyled as bordered pill shapes for improved visibility

#### Onboarding
- First-run setup wizard with Attio newsletter subscription step

#### Projects
- Yellow folder icon in the sidebar indicates projects with configuration drift
- Default Agents setting pre-populated into new projects

#### Settings
- Sub-page sidebar navigation for structured settings sections
- Default Agents global preference

#### Analytics
- Amplitude analytics integration with user opt-out support
- Analytics events routed through Rust backend to the Amplitude EU endpoint

#### CI / Distribution
- macOS code signing and notarization in the release workflow

### Fixes
- Analytics events now route correctly through the Rust backend to the EU endpoint
- Attio debug logging removed from newsletter integration
- Settings sub-page sidebar text brightened for legibility
- Error boundary and global error handlers added to surface black-screen failures
- ClerkProvider updated to allow Tauri origins; `.env.example` added

---

## [0.1.0] — 2026-02-27

Initial public release of Automatic — a desktop hub for AI coding agents.

### Core concepts

- **Hub, not executor** — Automatic does not run agents. It exposes an MCP server (stdio transport) that external tools (Claude Code, Cursor, custom agents) connect to in order to pull skills and sync configuration.
- **Skills** — reusable instruction sets with optional companion resources that agents load on demand via the MCP interface.
- **Projects** — workspace configurations that map a local directory to a set of agents, MCP servers, and skills.
- **Memory** — per-project key/value store that agents use to persist context across sessions.
- **Rules** — reusable content blocks that are injected into project instruction files.

### Features

#### Projects
- Three-step project creation wizard
- Auto-detection of installed agents (Claude Code, OpenCode, Codex, Cursor, Kiro, Goose, Warp, Antigravity, and more)
- Agent-specific SVG logos throughout the UI
- Editable project description and directory from the Summary tab
- Memory management tab per project
- One-click MCP server sync to agent config directories
- Skill sync with copy and symlink modes
- Inline editing of local skills within a project
- Unified project instructions and rules generation per template

#### Skills
- Full CRUD skill editor with frontmatter fields (name, description, tags)
- Companion resource discovery and display
- Skill Store integration — browse and install community skills from skills.sh
- Bundled marketplace template skills

#### MCP Marketplace
- Directory of 40 MCP servers with search and category filters
- One-click install into project configuration
- Brand icons via Brandfetch CDN
- Template dependency checking

#### Template Marketplace
- Browse and apply project templates
- Brand icons and indigo-unified theme

#### Dashboard
- Animated tech mesh background
- Getting Started section shown when no projects exist
- Marketplace shortcut cards
- Memory stat card in the project summary grid

#### Settings
- Skill sync mode configuration (copy vs symlink)
- Auto-update via `tauri-plugin-updater` — checks GitHub Releases for new versions, shows release notes, and prompts restart after install

#### MCP Server (agent interface)
- Five tools exposed over stdio transport: `list_skills`, `read_skill`, `list_projects`, `read_project`, `list_mcp_servers`
- Memory tools: `store_memory`, `get_memory`, `list_memories`, `search_memories`, `delete_memory`, `clear_memories`
- Credential retrieval: `get_credential`
- Session tracking: `list_sessions`
- `sync_project` tool — writes agent-specific MCP config files to the project directory

### Fixes
- Correct re-detection of Kiro, Goose, and Antigravity after agent removal
- Prevent removed agents from being re-added on project save/load
- Skill symlink now targets the skill directory, not individual files
- Skill fetch handles mismatched directory names
- Native Tauri dialog used for project deletion confirmation
- Warp removal correctly deletes `WARP.md` via owned config paths
- Junie removal deletes the entire `.junie/` directory

[0.2.0]: https://github.com/velvet-tiger/automatic/releases/tag/v0.2.0
[0.1.0]: https://github.com/velvet-tiger/automatic/releases/tag/v0.1.0
