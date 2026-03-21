# Automatic - Agent Context

## Core Concept

Automatic is a **hub, not an executor**. It does not run agents. External applications (Claude Code, Cursor, custom agents) connect to Automatic to:

- **Pull** skills and MCP server configs
- **Sync** project configurations to agent tool directories

Automatic exposes an **MCP Server** interface (stdio transport) that agents connect to.

## Quick Orientation

Automatic is a Tauri 2 desktop app (Rust + React/TypeScript). The Rust backend has four modules:

- **`src-tauri/src/core.rs`** -- Shared business logic (skills, projects, MCP config). Called by both Tauri commands and the MCP server.
- **`src-tauri/src/mcp.rs`** -- MCP server implementation using `rmcp` SDK. 5 tools exposed over stdio transport.
- **`src-tauri/src/lib.rs`** -- Tauri command wrappers (thin delegates to `core`) + app entry point.
- **`src-tauri/src/main.rs`** -- CLI dispatch: no args = Tauri app, `mcp-serve` = MCP server on stdio.

The React frontend:

- **`src/App.tsx`** -- Shell (sidebar + tab routing)
- **`src/Skills.tsx`**, **`src/Projects.tsx`** -- Configuration views
- **`src/Dashboard.tsx`** -- Overview dashboard
- **`src/Settings.tsx`** -- App settings with sub-page navigation

## What Exists

| Feature | Frontend | Backend | Status |
|---------|----------|---------|--------|
| Navigation shell | `App.tsx` | -- | Done |
| Dashboard | `Dashboard.tsx` | -- | Done |
| Skills CRUD | `Skills.tsx` | `core::list_skills`, `core::read_skill`, `core::save_skill`, `core::delete_skill` | Done |
| Skills Store (skills.sh) | `SkillStore.tsx` | `core::search_skills`, `core::install_skill` | Done |
| Projects CRUD | `Projects.tsx` | `core::list_projects`, `core::read_project`, `core::save_project`, `core::delete_project` | Done |
| Project Templates | `ProjectTemplates.tsx` | `core::list_project_templates`, `core::read_project_template`, `core::save_project_template` | Done |
| MCP Server config CRUD | `McpServers.tsx` | `core::list_mcp_server_configs`, `core::read_mcp_server_config`, `core::save_mcp_server_config`, `core::delete_mcp_server_config` | Done |
| MCP Server (stdio) | -- | `mcp.rs` (AutomaticMcpServer, 15+ tools) | Done |
| Agents view | `Agents.tsx` | `core::list_agents_with_projects` | Done |
| Rules CRUD | `Rules.tsx` | `core::list_rules`, `core::read_rule`, `core::save_rule`, `core::delete_rule` | Done |
| Instruction templates | `Templates.tsx` | `core::list_templates`, `core::read_template`, `core::save_template`, `core::delete_template` | Done |
| Settings | `Settings.tsx` | `core::read_settings`, `core::save_settings` | Done |
| First-run wizard | `FirstRunWizard.tsx` | `core::save_settings` | Done |
| Drift detection | `Projects.tsx` (banner) | `sync::check_project_drift` | Done |
| Analytics (Amplitude) | `analytics.ts` | `core::send_analytics_event` | Done |
| Auto-update | `Settings.tsx` | `tauri-plugin-updater` | Done |
| Memory tools | -- | `mcp.rs` (store/get/list/search/delete/clear) | Done |
| Project context module | -- | `context.rs` (implemented, not wired up) | Backend only |

## Making Changes

### Adding business logic

1. Add the function to `core.rs`
2. Add a `#[tauri::command]` wrapper in `lib.rs` that delegates to it
3. Register it in `generate_handler![]` in `lib.rs`
4. Call it from the frontend: `await invoke("name", { params })`

### Adding to marketplaces

When adding to one of the marketplaces, follow the dev-automatic-marketplace-authoring skill.

### Adding a frontend view

1. Create `src/NewView.tsx` as a functional component
2. Import it in `App.tsx`
3. Add a `<NavItem>` entry in the sidebar
4. Add a conditional render block: `{activeTab === "new-view" && <NewView />}`

### Build & verify

```bash
npm run build        # Frontend type check + bundle
cargo check          # Rust compilation check (from src-tauri/)
cargo test           # Run unit tests (from src-tauri/)
npm run tauri dev    # Full app with hot reload
```

<!-- automatic:groups:start -->
## Related Projects
The following projects are related to this one. They are provided for context — explore or reference them when relevant to the current task.

### Automatic
**automatic-webapp**
Location: `../automatic-webapp`
**deep-agents-rs**
Location: `../deep-agents-rs`

<!-- automatic:groups:end -->
