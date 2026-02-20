# Nexus - Agent Context

Read `.ai/constitution.md` before making any changes. It contains the full architecture, conventions, design system, and command reference.

## Quick Orientation

Nexus is a Tauri 2 desktop app (Rust + React/TypeScript). There are two codebases:

- **`src-tauri/src/lib.rs`** - The entire Rust backend. All Tauri commands live here. This is the only Rust source file that matters right now.
- **`src/`** - The React frontend. `App.tsx` is the shell (sidebar + routing). Feature views are separate components (`Skills.tsx`, `Providers.tsx`).

The frontend calls the backend with `invoke("command_name", { params })`. The backend returns `Result<T, String>`.

## What Exists

| Feature | Frontend | Backend | Status |
|---------|----------|---------|--------|
| Navigation shell | `App.tsx` | - | Done |
| LLM API key storage | `Providers.tsx` | `save_api_key`, `get_api_key` | Done |
| Skills CRUD | `Skills.tsx` | `get_skills`, `read_skill`, `save_skill`, `delete_skill` | Done |
| MCP config read | - | `get_mcp_servers` | Backend only |
| Local Agents | Empty state in `App.tsx` | - | Not started |
| Activity Log | Empty state in `App.tsx` | - | Not started |
| Settings | Empty state in `App.tsx` | - | Not started |

## Making Changes

### Adding a backend command

1. Write a `#[tauri::command] fn name(params) -> Result<T, String>` in `lib.rs`
2. Add it to the `generate_handler![]` macro at the bottom of `lib.rs`
3. Call it from the frontend: `await invoke("name", { params })`
4. Verify: `cargo check` from `src-tauri/`

### Adding a frontend view

1. Create `src/NewView.tsx` as a functional component
2. Import it in `App.tsx`
3. Add a `<NavItem>` entry in the sidebar
4. Add a conditional render block in the content area: `{activeTab === "new-view" && <NewView />}`

### Build & verify

```bash
npm run build        # Frontend type check + bundle
cargo check          # Rust compilation check (from src-tauri/)
npm run tauri dev    # Full app with hot reload
```
