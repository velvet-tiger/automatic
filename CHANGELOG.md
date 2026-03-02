# Changelog

All notable changes to Automatic are documented here.

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

[0.4.0]: https://github.com/velvet-tiger/automatic/releases/tag/v0.4.0
[0.3.0]: https://github.com/velvet-tiger/automatic/releases/tag/v0.3.0
[0.2.0]: https://github.com/velvet-tiger/automatic/releases/tag/v0.2.0
[0.1.0]: https://github.com/velvet-tiger/automatic/releases/tag/v0.1.0
