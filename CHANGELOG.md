# Changelog

All notable changes to Automatic are documented here.

## [0.9.0] — 2026-03-10

### Features

#### Features Tracking
- Per-project feature tracking with a full Kanban board UI and build list view
- MCP tools for agents to create, list, update, and claim features with assignee enforcement
- Build list columns are sortable; build filters persist per project
- Build view preference remembered per project
- New features open in the side panel for focused editing

#### Projects
- Project list sidebar auto-hides with a hover flyout and a pin toggle to keep it open
- Project tray opens on click (not hover) and closes automatically when unpinned after selection
- Project tabs grouped into collapsible sections for a cleaner layout

#### Skills
- Version-gated reinstall of bundled skills on app update ensures the latest built-in skill is always deployed
- Remote (store-installed) skills are now locked for editing; a Duplicate button creates an editable copy
- Bundled Automatic skill includes frontmatter and `skill.json` metadata

#### Recommendations
- System check added for missing `.automatic/context.json` to surface setup gaps early
- Two-phase AI inference approach guarantees structured JSON output from the recommendations engine

#### Conflicts
- Instruction file conflict modal now shows a full line-level diff for precise change review

### Fixes

- **Features**: Kanban drag-and-drop replaced HTML5 DnD with pointer-event handling to fix reliability on macOS
- **Recommendations**: Missing `metadata` column added to test schema, preventing DB errors
- **Skills**: `stopPropagation` call removed that was blocking clicks on GitHub links in skill cards
- **Projects**: OpenCode restart warning removed from the MCP Servers tab (no longer applicable)
- **Keychain**: Debug builds now use a separate keychain service name to avoid colliding with production credentials

### Tests

- 113 new unit tests added across 7 previously untested Rust modules

### Chores

- Skill Store: replaced Website Audit with Skill Creator in the featured skills list
- Updater: public key in `tauri.conf.json` refreshed

## [0.8.0] — 2026-03-08

### Features

#### Onboarding
- Anthropic API key step added to the first-run wizard with keychain storage and an obfuscated hint display

#### Dashboard
- Projects health bar added above the use cases section, showing overall project health at a glance

#### Projects
- Two-column summary layout with a rules widget and elevated setup callout
- Folder grouping with compact cards on the overview page

#### Context
- AI-powered project context generation with an integrated task log panel
- Project context exposed via a dedicated MCP tool (`get_project_context`)
- Context storage migrated from TOML to JSON; Tauri commands registered

#### Instructions
- AI generation for project instruction files with reactive recommendations
- Externally-modified instruction files are detected to prevent accidental overwrites
- Conflict modal simplified to a summary view with aligned `DriftReport` types

#### Rules
- Rules moved to a dedicated project tab with project-level scope
- Custom rule editor on the Rules tab with a dropdown global rule picker
- Inline custom project rules support
- Rules correctly routed to `.claude/rules/` in all write paths
- Selected rule indicators replaced solid brand fill with subtle highlights

#### Recommendations
- AI skill and MCP server suggestions with a proper install flow
- Compact single-line rows with a collapsible description toggle
- Recommendations now recompute on project save
- Rules recommendation copy updated to reflect the Rules tab

#### Task Log
- Task log entries persisted to disk
- Header toggle button and copy actions added

#### Agent / AI Playground
- API key management and live model list for AI Playground
- Library read and marketplace search tools added to the built-in agent
- Generate buttons gated on API key presence; environment variable fallback removed

#### Skills
- `skill.json` support added per the velvet-tiger/skills-json spec

#### Plugins
- Plugin framework introduced with a new Settings > Plugins page

#### Sidebar
- Navigation reorganised; Agents section renamed to Providers

### Fixes
- Theme: improved cyberpunk primary button text contrast

### Chore
- Version bumped to 0.7.0 with changelog (included in prior release)

---

## [0.7.0] — 2026-03-06

### Features

#### Projects
- Folder/group support in the project sidebar with overview tile sync
- Apply multiple templates to a project at once from the template selector

#### Markdown
- Blockquote rendering in MarkdownPreview components

### Fixes
- Wizard now deletes the stub project when the user cancels or navigates away
- Template rules are applied correctly even when `unified_instruction` is empty
- Sidebar and "New Project..." item are hidden while a project is being created

### Build
- Dev server port changed from 1420 to 1421

---

## [0.6.0] — 2026-03-05

### Features

#### Recommendations
- New system recommendations engine that evaluates projects for missing rules and instruction files, surfacing actionable items per project
- Dedicated Recommendations page (sidebar nav, Configuration section) grouped by project with dismiss support
- Per-project Recommendations tab in the Projects view
- Dashboard banner linking to the Recommendations page (replaces full inline list)
- SQLite-backed recommendations store with priority levels (low/normal/high) and full lifecycle management (pending/dismissed/actioned)

#### Memory
- Claude Code auto-memory integration: read-only access to `~/.claude/projects/<hash>/memory/` files directly in the Memory tab (requires Claude agent on project)
- Per-file Promote button to save Claude auto-memory entries into Automatic's structured memory store
- `automatic_read_claude_memory` MCP tool so agents can inspect Claude's learnings
- Memory mutations (store/delete/clear) now logged to the Activity feed and analytics

#### Agents
- Per-agent configuration options framework: inline collapsed settings panel per agent card with a chevron toggle
- `claude_rules_in_dot_claude` option (default: on) syncs rules to `.claude/rules/<name>.md` files instead of injecting inline into `CLAUDE.md`
- Default agent options configurable in Settings; new projects seeded from saved defaults

#### Projects
- Health bar summary strip above the card grid showing total projects, synced/drifted counts, unique agent count, skills in use, and MCP server count
- Segmented sync-health progress bar that fills as drift checks complete
- Project Instructions: Rules moved from footer strip to right sidebar

#### Skills
- Bundled `automatic-llms-txt` skill for creating `llms.txt` files following the llmstxt.org standard; auto-installed on first launch

### Fixes
- Drift detection now uses raw SKILL.md content for comparison, eliminating false positives
- Folder picker on macOS replaced with an `osascript`-backed command, fixing a panic on Apple Silicon caused by `NSOpenPanel` returning NULL (rfd #259)

### CI
- AMPLITUDE_API_KEY secret passed to Tauri build steps
- Workaround for tauri-action `latest.json` bug affecting universal macOS builds

---

## [0.5.0] — 2026-03-03

### Features

#### Onboarding Wizard
- Auto-detect installed agents and import global MCP server configs during first-run setup

#### Project Templates
- Software Defaults added as a bundled project template

#### Projects
- Search filter on the projects overview
- Sort toggle with last-activity ordering
- Inline MCP config preview with deep-link to the server detail page
- Redesigned project detail page layout

#### Activity
- Activity logging API and frontend integration with Recent Activity display on the dashboard

#### MCP Servers
- 67 additional servers from the Anthropic registry added to the MCP Marketplace, sorted alphabetically with contextual counts and expanded transport filters
- Environment variable values encrypted at rest; shell environment variable inheritance supported
- Per-row reveal toggle to mask/unmask env var values in the editor
- Beta notice banner shown on library MCP server detail views

#### Settings
- Reset and erase data actions

### Style
- Dashboard Discover & Extend cards aligned with use case card layout
- Animated TechMeshBackground removed from the dashboard
- OpenCode restart notice downgraded from warning to info

### Fixes
- Configuration dashboard updated to 3-column grid; MCP server sidebar border tokens corrected
- Window drag region restored during first-run wizard
- Card layout improved and status pill aligned in project card footer
- Dashboard row heights made consistent between Activity and Projects panels
- Sync button no longer navigates back to the projects list after syncing
- Empty state flash and badge height shift prevented on projects overview
- Dark theme icon colours corrected


---

## [0.4.0] — 2026-03-03

### Features

#### Skills
- Inline skill preview directly on the project Skills tab
- Fork-to-local action to copy a marketplace skill into the local skill library
- View-in-library shortcut to jump from a project skill to its Skills entry
- "Used By" right sidebar on skills showing linked projects and templates as clickable badges
- GitHub owner avatar displayed as skill logo with letter fallback
- License field support on skills
- Bundled Automatic skills with correct authorship attribution
- Core skills auto-installed on first run; template skills installed on demand

#### Project Templates
- "New project from template" action that opens the project wizard with the template pre-applied
- Applied To project badges on template detail pages are now clickable links
- Imported pill shown on template marketplace cards

#### Marketplaces
- Collections marketplace added, featuring the Automatic Skills collection
- Amplitude and Amplitude EU MCP servers added to the MCP Marketplace
- Marketplace-imported MCP server core settings are now locked to prevent accidental edits
- Skills marketplace renamed from "Skills Store" throughout the UI
- Consistent 3→4 column responsive grid across all three marketplaces
- Standardised search box layout across marketplaces

#### MCP Servers
- OAuth 2.1 PKCE proxy for authenticating remote MCP servers

#### Dashboard
- "How Automatic helps" use cases section added
- Featured section reworked with marketplace card template
- Getting Started section reorganised into a conditional right-column checklist
- Getting Started checklist flags persisted to `settings.json`
- Welcome message copy improved

#### Configuration
- Configuration Overview dashboard showing a summary of all configured resources

#### Projects
- Project overview replaced with full-width card grid layout
- Sync status badge pinned to the right edge of card headers

#### Theme
- Follow system light/dark preference by default
- Dark backgrounds lightened one zinc step for improved readability
- Always apply dark theme when disabling follow-system (no accidental light flash)
- Coral theme extended with a warm-tinted neutral palette
- Dark and corporate-dark theme backgrounds lightened; corporate-dark text brightened
- Muted text contrast improved in dark and cyberpunk themes

#### Developer
- Debug builds now use a separate `.automatic-dev` data directory to isolate dev state from production
- Minimal feature flag framework added (`flag()` helper in `flags.ts`)
- AI Playground view added (behind `ai_playground` feature flag)

### Fixes
- Stale marketplace plugin path resolved on fresh install
- Inner border removed from Getting Started items (fixes bottom border clipping)
- GitHub avatar fetch skipped for bundled skills (avoids 404 noise)
- Drift detection and sync now use a unified server map (eliminates false drift reports)
- Template `_author` propagated correctly on import and cleared on manual edit
- OpenCode MCP warning updated to note a restart is required for new servers
- MCP Marketplace "Add to MCP Servers" button uses white text in dark theme
- Folder icon colour on corporate-dark theme uses `icon-agent` token
- Traffic light position aligned to OS standard position
- Ship icon uses `icon-agent` colour token for theme consistency
- Projects sync badge pinned to right edge of card header
- Unused `sync_projects_referencing_rule` function removed

### Dependencies
- Switched HTTP backend to `native-tls` and removed unused dependencies

### CI
- `VITE_BRANDFETCH_CLIENT_ID` passed to `tauri-action` build steps

### Docs
- Windows build workaround documented for Rust on Parallels

### Chore
- `.claude-flow` daemon state and PID files removed from repository
- `github-release-management` skill removed from repository

---

## [0.3.0] — 2026-03-01

### Features

#### Themes
- Added Accessible theme with WCAG AA+ and colour-vision-deficiency-safe palette
- Added official Dark and Light themes as defaults
- Added Corporate Dark and Corporate Light themes (renamed from Sleek)
- Dynamic semantic icon colours with per-theme token mappings
- Dark agent icons rendered correctly in light themes

#### Dashboard
- Restructured layout with welcome note and featured cards using compact AuthorPanel layout

#### Author & Marketplace
- Author metadata added to marketplace templates, MCP servers, and skills
- AuthorPanel component with session caching and rate-limit fallback

#### Rules
- Per-project sync status indicators and Update buttons
- Automatic MCP Service rule renamed to "Automatic" and auto-enabled by default

#### Agents
- AgentCapabilities declaration for supported feature advertising
- Agent lists now sorted alphabetically by label

#### Memory
- Replaced inline memory tab with scalable MemoryBrowser component

#### Projects
- Summary tab redesigned with actionable layout

#### Onboarding Wizard
- "First project" step added to the onboarding flow
- Cancel support for re-opened wizard; mesh hidden on minimal themes

#### Editors
- JetBrains IDEs added to the "Open In" list

#### MCP Server
- Server metadata populated with title, description, and URL

#### UI
- App version displayed in sidebar footer
- Delete confirmation dialogs for MCP servers, templates, rules, and skills

### Refactoring
- Rust backend decomposed from monolithic `core.rs`/`lib.rs`/`sync.rs` into a modular directory structure
- Frontend localStorage keys migrated from `nexus.*` to `automatic.*` namespace
- All remaining Nexus references renamed to Automatic across the Rust backend

### Fixes
- AgentSelector Add button styling corrected to match other selectors
- Rules sync status check resolved for unified mode and section-only comparison
- Wizard step indicator layout restored; invalid border token classes removed
- Icon theme tokens used correctly in empty state icon boxes
- Agent icon filter corrected on light themes
- `allow-start-dragging` capability added to enable window drag
- Color contrast improved across light themes
- Markdown table borders lightened; inline code visibility fixed in Sleek theme
- Welcome link visibility improved on Corporate Dark
- Icon and rule pill contrast improved for Corporate Dark and Light themes

### CI
- Alternative Apple environment variables passed for notarization
- Keychain hang during codesign resolved

---

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

[0.8.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.8.0
[0.7.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.7.0
[0.6.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.6.0
[0.5.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.5.0
[0.4.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.4.0
[0.3.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.3.0
[0.2.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.2.0
[0.1.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.1.0
