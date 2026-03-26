# Automatic — AI Agent Instruction File

## Project Overview

**Automatic** is a desktop hub for managing AI agent configuration across projects. It provides centralized storage and synchronization of skills, MCP servers, rules, templates, and project instructions for agent tools like Claude Code, Codex CLI, Cursor, and other MCP-compatible systems.

**Tech Stack:**
- **Frontend:** React 19, TypeScript, Tailwind CSS 4, Vite 7
- **Backend:** Rust (Tauri 2), rmcp SDK for MCP protocol
- **Build System:** Tauri CLI, npm scripts, Makefile
- **Storage:** File-based JSON configuration in `~/.agents/` (no database)

The application runs in two modes:
1. **GUI mode** (default) — desktop UI for managing configuration
2. **MCP server mode** (`mcp-serve` CLI arg) — stdio-based Model Context Protocol server exposing 15+ tools for external agents

## Build & Run Commands

**Development:**
```bash
make dev                # Run Tauri app with hot reload
npm run dev             # Frontend only (Vite dev server)
```

**Build:**
```bash
make build              # Full Tauri production build
npm run build           # Frontend TypeScript + Vite bundle
```

**Checks & Tests:**
```bash
make check              # Run all checks (frontend + Rust)
npm run lint            # TypeScript type checking only
cd src-tauri && cargo test   # Rust unit tests
cd src-tauri && cargo clippy -- -D warnings   # Rust linting
```

**Other:**
```bash
make install            # Install npm + cargo dependencies
make clean              # Remove dist/, target/, node_modules/
npm run tauri [cmd]     # Direct Tauri CLI access
```

## Architecture Overview

**Frontend (React + Tauri):**
- `src/main.tsx` — App entry point, mounts React into Tauri window
- `src/App.tsx` — Tab-based navigation shell (Skills, Projects, Dashboard, Settings)
- `src/components/` — Reusable UI components (AgentSelector, SkillSelector, MarkdownPreview, etc.)
- `src/contexts/` — React context providers (ProfileContext, TaskLogContext, UpdateContext)
- `src/pages/` — Top-level page components (FirstRunWizard, GettingStarted, Recommendations, Settings, marketplace/*, utilities/*, workspace/*)
- `src/plugins/` — Plugin system registry and hooks (ToolPanelRegistry.ts, usePlugin.ts)
- `src/lib/` — Shared utilities (analytics.ts, theme.ts, flags.ts, icons.ts)

**Backend (Rust + Tauri):**
- `src-tauri/src/main.rs` — Dual-mode entry point (GUI or MCP stdio server)
- `src-tauri/src/lib.rs` — Tauri command registration and public API
- `src-tauri/src/mcp.rs` — MCP server implementation using rmcp SDK, exposes 15+ tools via stdio
- `src-tauri/src/core/` — Core business logic for skills, projects, MCP servers, rules, templates
- `src-tauri/src/commands/` — Thin Tauri command wrappers that delegate to core logic
- `src-tauri/src/sync/` — Project sync engine and drift detection
- `src-tauri/src/context.rs` — Context generation for agent instructions
- `src-tauri/src/memory.rs` — Key-value memory storage for project-specific context
- `src-tauri/src/agent/` — Agent type definitions and sync target logic

**Data & Assets:**
- `src-tauri/skills/` — Bundled skill definitions (automatic-*, laravel-specialist, php-pro, etc.)
- `src-tauri/rules/` — Bundled rule templates
- `src-tauri/agents/` — Agent-specific templates and config structures
- `src-tauri/templates/` — Markdown templates (Agent Project Brief, Session Context)
- `src-tauri/languages/` — Language-specific module definitions (.mod files)
- `src-tauri/featured-mcp-servers.json` — Curated MCP server registry
- `src-tauri/collections.json` — Skill/server collection definitions

**User Data Storage:**
- All runtime configuration stored in `~/.agents/` (file-based, no database)
- Drift detection compares in-memory config with on-disk files in project directories

## Coding Conventions

**TypeScript (Frontend):**
- **Strict mode enabled** — all props and Tauri invoke parameters must be fully typed, no `any` types
- **Functional components** — use `useState`/`useEffect`, no class components
- **Tailwind CSS** — use design tokens from `src/lib/theme.ts`, no arbitrary inline values
- **Tauri invokes** — call backend commands via `@tauri-apps/api`, command names must match Rust exactly
- **File naming** — PascalCase for components (`AgentSelector.tsx`), camelCase for utilities (`analytics.ts`)

**Rust (Backend):**
- **Thin command layer** — Tauri commands in `lib.rs` are wrappers that delegate to `core.rs`, no business logic in command handlers
- **Visibility boundaries** — use `pub(crate)` for internal module APIs, expose only through `lib.rs` and `mcp.rs`
- **Macro-driven MCP tools** — use `#[tool]` macro for auto-discovery, param structs must derive `Deserialize`, `Serialize`, `JsonSchema`
- **Error handling** — return `Result<T, String>` from commands, propagate context with `.map_err(|e| format!("context: {}", e))`
- **File operations** — all config reads/writes go through `~/.agents/` directory structure, use `std::fs` with proper error handling

**General Patterns:**
- **No database** — all state is file-based JSON, stored in user config directories
- **Stateless frontend** — no Redux/MobX, component state only, invoke Tauri commands directly
- **Drift detection** — when syncing projects, compare in-memory config with on-disk files and alert on divergence
- **Skill sync modes** — skills can be symlinked or copied to project directories (global setting in `~/.agents/config.json`)
- **Analytics opt-in** — check `flags.ts` for feature gates before sending events to Amplitude

## Agent Guidance

**What the Agent Should Do:**
- **Always run `make check`** before committing changes (validates TypeScript + Rust compilation)
- **Run `cargo test`** after modifying Rust backend logic
- **Read existing code patterns** before generating new components — match project style
- **Use MCP tools** to pull project context, skills, and memory when working on Automatic-managed projects
- **Follow the Agent Constitution** in `AGENTS.md` (phases: Understand → Context → Plan → Implement → Verify → Communicate)
- **Declare gaps** — if external context is missing (API schemas, env secrets, unseen dependencies), stop and ask
- **Minimal scope changes** — edit only what is relevant to the task, avoid refactoring unrelated code
- **Document decisions** — capture architectural choices, gotchas, and conventions in memory using MCP tools

**What the Agent Should Not Do:**
- **Never commit secrets or credentials** — check `.env.example` for environment variable patterns
- **Never delete files without confirmation** — especially user data in `~/.agents/` or bundled skills/rules
- **Never assume MCP server paths** — current implementation hardcodes macOS Claude Desktop paths, cross-platform support is pending
- **Never change Tauri command names** without updating frontend invokes — name coupling is strict
- **Never send analytics events** without checking opt-in status in `flags.ts`
- **Never ship code with placeholders** — mark `TODO` comments clearly, do not claim incomplete work is done
- **Do not loop on failures** — if the same error repeats 3+ times, stop and report the blocker with diagnostics

**Before Starting Work:**
1. Call `automatic_read_project` to load project configuration
2. Call `automatic_list_skills` and read relevant skills with `automatic_read_skill`
3. Call `automatic_search_memories` for project-specific context (conventions, decisions, gotchas)
4. Confirm task scope and constraints before writing code

**Before Finishing Work:**
1. Run `make check` and `cargo test`
2. Call `automatic_store_memory` to persist new learnings, conventions, or decisions
3. Summarize changes, declare out-of-scope items, and flag any uncertainties

**Gotchas to Watch:**
- **Dual-mode entry point** — `main.rs` dispatches GUI or MCP server based on CLI args, do not break this branching
- **Code signing required** — macOS builds need signing for auto-updater to work, unsigned builds fail update checks
- **Skill sync mode switching** — changing global sync mode (symlink vs copy) mid-project can confuse users, warn if switching
- **Drift alerts** — manually editing synced files triggers drift detection until re-synced, this is expected behavior
- **Command name coupling** — frontend TypeScript and Rust command names must match exactly, typos break invokes silently

<!-- automatic:groups:start -->
## Related Projects
The following projects are related to this one. They are provided for context — explore or reference them when relevant to the current task.

### Automatic
**automatic-webapp**
Location: `../automatic-webapp`
**deep-agents-rs**
Location: `../deep-agents-rs`

<!-- automatic:groups:end -->
