# Automatic - Agent Context

Read `.ai/constitution.md` before making any changes. It contains the full architecture, conventions, design system, and command reference.

## Core Concept

Automatic is a **hub, not an executor**. It does not run agents. External applications (Claude Code, Cursor, custom agents) connect to Automatic to:

- **Pull** credentials, skills, and MCP server configs
- **Sync** project configurations to agent tool directories

Automatic exposes an **MCP Server** interface (stdio transport) that agents connect to.

## Quick Orientation

Automatic is a Tauri 2 desktop app (Rust + React/TypeScript). The Rust backend has four modules:

- **`src-tauri/src/core.rs`** -- Shared business logic (credentials, skills, projects, MCP config). Called by both Tauri commands and the MCP server.
- **`src-tauri/src/mcp.rs`** -- MCP server implementation using `rmcp` SDK. 5 tools exposed over stdio transport.
- **`src-tauri/src/lib.rs`** -- Tauri command wrappers (thin delegates to `core`) + app entry point.
- **`src-tauri/src/main.rs`** -- CLI dispatch: no args = Tauri app, `mcp-serve` = MCP server on stdio.

The React frontend:

- **`src/App.tsx`** -- Shell (sidebar + tab routing)
- **`src/Skills.tsx`**, **`src/Projects.tsx`**, **`src/Providers.tsx`** -- Configuration views

## What Exists

| Feature | Frontend | Backend | Status |
|---------|----------|---------|--------|
| Navigation shell | `App.tsx` | -- | Done |
| LLM API key storage | `Providers.tsx` | `core::save_api_key`, `core::get_api_key` | Done |
| Skills CRUD | `Skills.tsx` | `core::list_skills`, `core::read_skill`, `core::save_skill`, `core::delete_skill` | Done |
| Projects CRUD | `Projects.tsx` | `core::list_projects`, `core::read_project`, `core::save_project`, `core::delete_project` | Done |
| MCP config read | Used in `Projects.tsx` | `core::list_mcp_servers` | Done |
| MCP Server | -- | `mcp.rs` (5 tools, stdio) | Done |
| MCP Server config editing | Empty state in `App.tsx` | -- | Not started |
| Settings | Placeholder in `App.tsx` | -- | Not started |

## Making Changes

### Adding business logic

1. Add the function to `core.rs`
2. Add a `#[tauri::command]` wrapper in `lib.rs` that delegates to it
3. Register it in `generate_handler![]` in `lib.rs`
4. Call it from the frontend: `await invoke("name", { params })`

### Exposing a new MCP tool

1. Define a params struct with `#[derive(Deserialize, Serialize, JsonSchema)]` in `mcp.rs`
2. Add a `#[tool]` method in the `NexusMcpServer` impl block (under `#[tool_router]`)
3. Use `Parameters<T>` wrapper for the tool's input parameter
4. The tool router picks it up automatically -- no manual registration needed

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
