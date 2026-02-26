# Automatic - Project Constitution

## Identity

Automatic is a cross-platform desktop application that serves as a **registry and configuration hub** for AI agent tooling. It does not execute agents itself. Instead, external applications (Claude Code, OpenCode, custom scripts, etc.) connect to Automatic to discover skills, read MCP configurations, and sync project configs.

Automatic exposes its services via an **MCP Server** interface for AI-native tools that already speak MCP (Claude Code, Cursor, etc.).

The desktop UI (Tauri) provides a visual management layer for editing, browsing, and monitoring everything Automatic serves.

## Architecture

### Three-Layer Model

```
┌─────────────────────────────────────────────────┐
│  Tauri Desktop UI (React/TypeScript)            │  Human interface
│  - Edit skills, providers, MCP configs          │  for configuration
│  - Manage projects and sync configs             │  and management
├─────────────────────────────────────────────────┤
│  Rust Core (src-tauri/src/core.rs)              │  Shared logic:
│  - Filesystem operations                        │  filesystem,
│  - Validation & path safety                     │  validation, data access
├─────────────────────────────────────────────────┤
│  Service Layer                                  │  External-facing API
│  - MCP Server (src-tauri/src/mcp.rs)            │  that agents
│    stdio transport, 5 tools                     │  connect to
└─────────────────────────────────────────────────┘
        ▲               ▲               ▲
        │               │               │
   Claude Code      Cursor         Custom agents
   (via MCP)        (via MCP)      (via MCP)
```

The Rust core is shared between the Tauri UI layer and the MCP server. Both call the same internal functions for skill reading, project access, etc.

### Data Flow

Automatic is **passive by default**. It serves data on request:

**Outbound (Automatic serves to external apps):**
- Skills: "List available skills" / "Read skill X"
- MCP config: "What MCP servers should I connect to?"
- Project configs: "What's the configuration for project Y?"

Automatic does NOT:
- Spawn or manage agent processes
- Proxy or mediate LLM API calls
- Execute tools on behalf of agents

### Key Directories

```
src/                  # React/TypeScript frontend (Tauri webview)
  App.tsx             # Root layout: sidebar navigation + content routing
  App.css             # Global styles, Tailwind import, custom scrollbar
  Skills.tsx          # Skills list + markdown editor (two-pane view)
  Projects.tsx        # Project CRUD with skills/MCP/provider assignment
  main.tsx            # React DOM entry point

src-tauri/            # Rust backend
  src/
    main.rs           # Binary entry point: dispatches to Tauri or MCP server
    lib.rs            # Tauri command wrappers + app entry
    core.rs           # Shared business logic (skills, projects, MCP config)
    mcp.rs            # MCP server implementation (rmcp SDK, stdio transport, 5 tools)
  Cargo.toml          # Rust dependencies
  capabilities/       # Tauri permission capabilities
  icons/              # App icons for all platforms
```

### Data Storage

| Data | Location | Mechanism |
|------|----------|-----------|
| Skills | `~/.agents/skills/<name>/SKILL.md` (primary, agentskills.io standard) + `~/.claude/skills/<name>/SKILL.md` (Claude Code) | Filesystem |
| Projects | `<project-dir>/.automatic/project.json` (full config, pretty-printed); `~/.automatic/projects/<name>.json` (registry entry pointing to directory) | Filesystem (JSON) |
| MCP config | `~/Library/Application Support/Claude/claude_desktop_config.json` | Read-only, Mac only |

### Frontend-Backend Communication (Tauri UI)

The React frontend calls the Rust backend using `invoke()` from `@tauri-apps/api/core`. Every backend function is a `#[tauri::command]` in `lib.rs` that delegates to `core.rs`.

Current commands:

| Command | Params | Returns | Purpose |
|---------|--------|---------|---------|
| `get_skills` | none | `Result<Vec<SkillEntry>>` | List skills from `~/.agents/skills/` and `~/.claude/skills/` with location flags |
| `read_skill` | `name` | `Result<String>` | Read `SKILL.md` content (checks `~/.agents/skills/` first, then `~/.claude/skills/`) |
| `save_skill` | `name, content` | `Result<()>` | Write `SKILL.md` to `~/.agents/skills/` (the standard location) |
| `delete_skill` | `name` | `Result<()>` | Remove skill from both global locations |
| `sync_skill` | `name` | `Result<()>` | Copy a skill across both global directories |
| `sync_all_skills` | none | `Result<Vec<String>>` | Sync all unsynced skills across both global directories |
| `get_mcp_servers` | none | `Result<String>` | Read `claude_desktop_config.json` (Mac path) |
| `get_projects` | none | `Result<Vec<String>>` | List project names from `~/.automatic/projects/` |
| `read_project` | `name` | `Result<String>` | Read project JSON |
| `save_project` | `name, data` | `Result<()>` | Validate and write project JSON |
| `delete_project` | `name` | `Result<()>` | Delete project file |

All commands must be registered in the `tauri::generate_handler![]` macro in `lib.rs`.

### MCP Server Interface

Automatic runs as an MCP server via `nexus mcp-serve`. The server uses the `rmcp` SDK (official Rust MCP implementation) with stdio transport. It exposes Automatic capabilities as MCP tools:

| MCP Tool | Parameters | Description |
|----------|------------|-------------|
| `nexus_list_skills` | none | List all available skill names |
| `nexus_read_skill` | `name: String` | Read the content of a specific skill |
| `nexus_list_mcp_servers` | none | Get MCP server configurations |
| `nexus_sync_project` | `name: String` | Sync a project's MCP configs to its directory |

An external tool configures this in its MCP settings:
```json
{
  "mcpServers": {
    "automatic": {
      "command": "nexus",
      "args": ["mcp-serve"]
    }
  }
}
```

### Binary Entry Point

`main.rs` dispatches based on the first CLI argument:
- **No args** (default): launches the Tauri desktop app
- **`mcp-serve`**: launches the MCP server on stdio (no UI)



## Conventions

### Rust Backend

- **Module separation**: Business logic lives in `core.rs`. Tauri commands in `lib.rs` are thin wrappers that delegate to `core::` functions. The MCP server in `mcp.rs` calls the same `core::` functions. Sync logic lives in `sync.rs`. Do not duplicate logic.
- All Tauri-facing functions must be `#[tauri::command]` and return `Result<T, String>`.
- Use `.map_err(|e| e.to_string())` to convert errors for the frontend.
- Validate all user-provided path components with `is_valid_name()` before any filesystem operation. Never allow path traversal.
- Use the `dirs` crate for resolving home directories. Do not hardcode paths.
- Skills are stored at `~/.agents/skills/<name>/SKILL.md` (primary, agentskills.io standard) and also scanned from `~/.claude/skills/<name>/SKILL.md` (Claude Code). New skills are saved to `~/.agents/skills/` by default.
- MCP config is read from `~/Library/Application Support/Claude/claude_desktop_config.json` (Mac). Cross-platform support is pending.
- MCP tools use `rmcp` macros: `#[tool]` for tool functions, `#[tool_router]` on the impl block, `#[tool_handler]` on the `ServerHandler` impl.
- MCP tool parameters use `Parameters<T>` wrapper from `rmcp::handler::server::wrapper`.
- The `schemars` crate used by rmcp is v1 (re-exported as `rmcp::schemars`). The project also has `schemars` v0.8 from Tauri. Use `use rmcp::schemars;` in `mcp.rs` so the `JsonSchema` derive macro resolves correctly.

### React Frontend

- Use functional components with hooks. No class components.
- State management is local (`useState`/`useEffect`). No external state library yet.
- All backend calls go through `invoke()` from `@tauri-apps/api/core`.
- Icons come from `lucide-react`. Import only what you need.
- The app uses Tailwind CSS v4 (via `@tailwindcss/vite` plugin). No `tailwind.config.js` file; configuration is done via CSS.

### Design System (Linear-inspired Dark Theme)

The UI follows a dark, developer-focused aesthetic inspired by Linear. These are the core design tokens used as inline Tailwind values:

| Token | Hex | Usage |
|-------|-----|-------|
| Background (main) | `#222327` | Primary content area |
| Background (sidebar) | `#1A1A1E` | Left navigation sidebar |
| Surface (hover/active) | `#2D2E36` | Active nav items, hover states |
| Border | `#33353A` | All dividers and borders |
| Border (hover) | `#44474F` | Input hover borders |
| Text (primary) | `#E0E1E6` | Headings, body text |
| Text (secondary) | `#8A8C93` | Labels, descriptions, muted text |
| Accent (purple) | `#5E6AD2` | Primary buttons, focus rings |
| Accent (purple hover) | `#6B78E3` | Button hover state |
| Destructive | `#FF6B6B` | Delete actions |
| Status (active) | `#4ADE80` | Active agent indicator |
| Status (idle) | `#FACC15` | Idle agent indicator |

Typography:
- Font: Inter (loaded via Google Fonts in `index.html`)
- Nav items: `text-[13px] font-medium`
- Section headers: `text-[11px] font-semibold tracking-wider uppercase`
- Body: `text-[14px]`
- Code/editor: `font-mono text-[13px]`

Layout patterns:
- Sidebar is always `w-[240px]` fixed width.
- Headers are `h-11` or `h-12` with bottom borders.
- Two-pane views use `w-[256px]` left pane with a list + right pane with detail.
- Empty states use a dashed circle icon + heading + description + primary action button.
- Use `custom-scrollbar` class for styled scrollbars (defined in `App.css`).

### Adding a New Feature

1. Add business logic to `core.rs`.
2. Add a `#[tauri::command]` wrapper in `lib.rs` that delegates to the core function. Register it in `generate_handler![]`.
3. If the feature should be exposed to agents, add a `#[tool]` method in `mcp.rs` in the `NexusMcpServer` impl block.
4. Create a new React component in `src/` (e.g., `NewView.tsx`).
5. Import it in `App.tsx` and add a tab entry in the sidebar navigation under the appropriate category.
6. Follow the existing pattern: the tab content is rendered conditionally based on `activeTab` state.

### Commands

```bash
npm install          # Install frontend dependencies
npm run dev          # Start Vite dev server (frontend only)
npm run build        # TypeScript check + Vite production build
npm run tauri dev    # Full dev mode: Rust backend + React frontend + hot reload
npm run tauri build  # Production build: compiles .app / .exe / .deb
cargo check          # Check Rust compilation (from src-tauri/)
cargo test           # Run unit tests (from src-tauri/)
```

## Status & Roadmap

### Implemented
- Sidebar navigation with Linear-style dark theme
- Skills management (list, create, read, edit, delete from `~/.claude/skills/`)
- Project management (CRUD with skills/MCP/provider assignment)
- MCP server config reading (read-only, Mac only)
- Core logic extraction (`core.rs` -- shared between Tauri and MCP server)
- MCP Server (stdio transport, 5 tools via `rmcp` SDK)
- Project config sync to agent tool directories (Claude Code, Codex CLI, OpenCode)
- CLI dispatch (`nexus` = Tauri app, `nexus mcp-serve` = MCP server)

### Next
- MCP server configuration editing and creation
- Settings / Preferences page
- Cross-platform MCP config paths (currently Mac-only)

### Future
- Remote MCP server (SSE transport) with OAuth 2.1 authentication
- Integration with auth providers like Prefactor for MCP auth
- Agent-initiated skill discovery

## Gotchas

- The `package.json` contains `@nexuss/api` and `@nexuss/cli` entries. These are the renamed Tauri npm packages that resulted from a global find-replace of "tauri" to "nexus". They still resolve correctly via npm because the underlying packages are `@tauri-apps/api` and `@tauri-apps/cli`. If you run into resolution issues, check `package-lock.json`.
- Vite warns about Node.js version requirements (`^20.19.0 || >=22.12.0`). The project currently works on Node 20.16.0 but this may break in future Vite versions.
- The Rust crate is named `nexus` with lib name `nexus_lib`. The binary entry point in `main.rs` calls `nexus_lib::run()`.
- `rmcp` v0.16 uses `schemars` v1 while Tauri uses `schemars` v0.8. Both coexist as separate crate versions. In `mcp.rs`, always use `use rmcp::schemars;` to get the correct version for `#[derive(JsonSchema)]`.

