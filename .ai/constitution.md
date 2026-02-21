# Nexus - Project Constitution

## Identity

Nexus is a cross-platform desktop application that serves as a **registry, credential vault, and configuration hub** for AI agent tooling. It does not execute agents itself. Instead, external applications (Claude Code, OpenCode, custom scripts, etc.) connect to Nexus to retrieve credentials, discover skills, read MCP configurations, and optionally report their status.

Nexus exposes its services via two interfaces:
1. **MCP Server** -- for AI-native tools that already speak MCP (Claude Code, OpenCode).
2. **Local HTTP API** -- for scripts, CLIs, and non-MCP clients.

The desktop UI (Tauri) provides a visual management layer for editing, browsing, and monitoring everything Nexus serves.

## Architecture

### Three-Layer Model

```
┌─────────────────────────────────────────────────┐
│  Tauri Desktop UI (React/TypeScript)            │  Human interface
│  - Edit skills, providers, MCP configs          │  for configuration
│  - Monitor connected agents                     │  and monitoring
├─────────────────────────────────────────────────┤
│  Rust Core (src-tauri/src/lib.rs)               │  Shared logic:
│  - OS keychain access                           │  filesystem, keychain,
│  - Filesystem operations                        │  validation, data access
│  - Validation & path safety                     │
├─────────────────────────────────────────────────┤
│  Service Layer (planned)                        │  External-facing APIs
│  - MCP Server (stdio or SSE transport)          │  that other apps
│  - Local HTTP API (localhost:PORT)              │  connect to
└─────────────────────────────────────────────────┘
        ▲               ▲               ▲
        │               │               │
   Claude Code      OpenCode      Custom scripts
   (via MCP)        (via MCP)     (via HTTP)
```

The Rust core is shared between the Tauri UI layer and the service layer. Both call the same internal functions for keychain access, skill reading, etc.

### Data Flow

Nexus is **passive by default**. It serves data on request:

**Outbound (Nexus serves to external apps):**
- Credentials: "Give me the API key for Anthropic"
- Skills: "List available skills" / "Read skill X"
- MCP config: "What MCP servers should I connect to?"
- Project configs: "What's the configuration for project Y?"

**Inbound (external apps report to Nexus):**
- Agent registration: "I'm agent X, running on project Y"
- Status updates: "Session started / completed / errored"
- Activity events: "Tool call executed", "tokens used"

Nexus does NOT:
- Spawn or manage agent processes
- Proxy or mediate LLM API calls
- Execute tools on behalf of agents

### Key Directories

```
src/                  # React/TypeScript frontend (Tauri webview)
  App.tsx             # Root layout: sidebar navigation + content routing
  App.css             # Global styles, Tailwind import, custom scrollbar
  Skills.tsx          # Skills list + markdown editor (two-pane view)
  Providers.tsx       # LLM API key management form
  main.tsx            # React DOM entry point

src-tauri/            # Rust backend
  src/lib.rs          # All Tauri commands AND shared core logic
  src/main.rs         # Binary entry point (calls nexus_lib::run())
  Cargo.toml          # Rust dependencies
  tauri.conf.json     # Window config, build config, bundle config
  capabilities/       # Tauri permission capabilities
  icons/              # App icons for all platforms
```

### Frontend-Backend Communication (Tauri UI)

The React frontend calls the Rust backend using `invoke()` from `@tauri-apps/api/core`. Every backend function is a `#[tauri::command]` in `lib.rs`.

Current commands:

| Command | Params | Returns | Purpose |
|---------|--------|---------|---------|
| `save_api_key` | `provider: str, key: str` | `Result<()>` | Store API key in OS keychain |
| `get_api_key` | `provider: str` | `Result<String>` | Retrieve API key from OS keychain |
| `get_skills` | none | `Result<Vec<String>>` | List skill directory names from `~/.claude/skills/` |
| `read_skill` | `name: str` | `Result<String>` | Read `SKILL.md` content from a skill directory |
| `save_skill` | `name: str, content: str` | `Result<()>` | Write `SKILL.md`, creating directory if needed |
| `delete_skill` | `name: str` | `Result<()>` | Remove entire skill directory |
| `get_mcp_servers` | none | `Result<String>` | Read `claude_desktop_config.json` (Mac path) |

All commands must be registered in the `tauri::generate_handler![]` macro in `lib.rs`.

### External Service Interfaces (Planned)

#### MCP Server

Nexus will run an MCP server that AI tools can add to their MCP configuration. The server exposes Nexus capabilities as MCP tools:

| MCP Tool | Description |
|----------|-------------|
| `nexus_get_credential` | Retrieve an API key by provider name |
| `nexus_list_skills` | List all available skill names |
| `nexus_read_skill` | Read the content of a specific skill |
| `nexus_list_mcp_servers` | Get MCP server configurations |
| `nexus_register_agent` | Register an external agent session |
| `nexus_report_status` | Push a status update from an external agent |

An external tool would configure this in its MCP settings:
```json
{
  "mcpServers": {
    "nexus": {
      "command": "nexus",
      "args": ["mcp-serve"]
    }
  }
}
```

#### HTTP API

A local HTTP server (e.g. `http://localhost:7400`) for non-MCP clients:

```
GET  /api/credentials/:provider
GET  /api/skills
GET  /api/skills/:name
GET  /api/mcp-servers
POST /api/agents/register
POST /api/agents/:id/status
```

## Conventions

### Rust Backend

- All Tauri-facing functions must be `#[tauri::command]` and return `Result<T, String>`.
- Use `.map_err(|e| e.to_string())` to convert errors for the frontend.
- Validate all user-provided path components with `is_valid_skill_name()` or equivalent before any filesystem operation. Never allow path traversal.
- API keys are stored via the `keyring` crate under the service name `nexus_desktop`.
- Use the `dirs` crate for resolving home directories. Do not hardcode paths.
- Skills are stored at `~/.claude/skills/<name>/SKILL.md`.
- MCP config is read from `~/Library/Application Support/Claude/claude_desktop_config.json` (Mac). Cross-platform support is pending.
- When the MCP server and HTTP API are implemented, they must call the same core functions that the Tauri commands use. Do not duplicate logic.

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

Typography:
- Font: Inter (loaded via Google Fonts in `index.html`)
- Nav items: `text-[13px] font-medium`
- Section headers: `text-[11px] font-semibold tracking-wider uppercase`
- Body: `text-[14px]`
- Code/editor: `font-mono text-[13px]`

Layout patterns:
- Sidebar is always `w-[240px]` fixed width.
- Headers are `h-11` or `h-12` with bottom borders.
- Empty states use a dashed circle icon + heading + description + primary action button.
- Use `custom-scrollbar` class for styled scrollbars (defined in `App.css`).

### Adding a New Feature

1. If the feature needs OS access (files, keychain, processes), add a `#[tauri::command]` to `lib.rs` and register it in `generate_handler![]`.
2. Create a new React component in `src/` (e.g., `McpServers.tsx`).
3. Import it in `App.tsx` and add a tab entry in the sidebar navigation under the appropriate category.
4. Follow the existing pattern: the tab content is rendered conditionally based on `activeTab` state.

### Commands

```bash
npm install          # Install frontend dependencies
npm run dev          # Start Vite dev server (frontend only)
npm run build        # TypeScript check + Vite production build
npm run tauri dev    # Full dev mode: Rust backend + React frontend + hot reload
npm run tauri build  # Production build: compiles .app / .exe / .deb
cargo check          # Check Rust compilation (run from src-tauri/)
```

## Status & Roadmap

### Implemented
- Sidebar navigation with Linear-style dark theme
- LLM Provider key management (save/load from OS keychain)
- Skills management (list, create, read, edit, delete from `~/.claude/skills/`)
- MCP server config reading (read-only, Mac only)

### Next: Service Layer
- Extract core logic in `lib.rs` into reusable functions (separate from `#[tauri::command]` wrappers)
- Implement MCP server (stdio transport) exposing credentials, skills, MCP configs
- Implement local HTTP API on localhost for non-MCP clients
- Add agent registration and status reporting endpoints

### Later: UI Enhancements
- Connected Agents view (shows agents that have registered via MCP/HTTP)
- Activity log (streaming events from connected agents)
- MCP server configuration editing and creation
- Settings / Preferences page
- Cross-platform MCP config paths (currently Mac-only)

## Gotchas

- The `package.json` contains `@nexuss/api` and `@nexuss/cli` entries. These are the renamed Tauri npm packages that resulted from a global find-replace of "tauri" to "nexus". They still resolve correctly via npm because the underlying packages are `@tauri-apps/api` and `@tauri-apps/cli`. If you run into resolution issues, check `package-lock.json`.
- Vite warns about Node.js version requirements (`^20.19.0 || >=22.12.0`). The project currently works on Node 20.16.0 but this may break in future Vite versions.
- The Rust crate is named `nexus` with lib name `nexus_lib`. The binary entry point in `main.rs` calls `nexus_lib::run()`.
