# Automatic

A desktop application for managing LLM provider connections, running local AI agents, and editing Skills and MCP server configurations. Built with Tauri 2 (Rust) and React/TypeScript.

## Prerequisites

- [Node.js](https://nodejs.org/) >= 20.x
- [Rust](https://rustup.rs/) (stable)
- Platform build dependencies for Tauri: see [Tauri Prerequisites](https://v2.tauri.app/start/prerequisites/)

## Quick Start

```bash
npm install
npm run tauri dev
```

This compiles the Rust backend and launches the app with hot-reloading for the React frontend.

## Build

```bash
npm run tauri build
```

Produces a native `.app` (macOS), `.msi` (Windows), or `.deb`/`.AppImage` (Linux) in `src-tauri/target/release/bundle/`.

## Project Structure

```
src/                  # React frontend (TypeScript, Tailwind CSS v4)
  App.tsx             # Shell: sidebar navigation + tab routing
  Skills.tsx          # Skills list + markdown editor (two-pane)
  Providers.tsx       # LLM API key management

src-tauri/            # Rust backend
  src/lib.rs          # All Tauri commands (backend API surface)
  src/main.rs         # Binary entry point
  tauri.conf.json     # Window config, bundling, identifiers

.ai/constitution.md   # Full architecture docs, conventions, design tokens
AGENTS.md             # Quick-start context for LLM agents
```

## Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust, Tauri 2 |
| Frontend | React 19, TypeScript, Vite 7 |
| Styling | Tailwind CSS v4 |
| Icons | lucide-react |
| Key Storage | OS keychain via `keyring` crate |
| Skills Storage | `~/.claude/skills/<name>/SKILL.md` (filesystem) |

## Current Features

- **LLM Providers** -- Save and retrieve API keys securely via OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service).
- **Skills Management** -- Full CRUD for agent skills stored at `~/.claude/skills/`. Two-pane UI with list + markdown editor.
- **MCP Config Reading** -- Reads `claude_desktop_config.json` for MCP server definitions (read-only, Mac path).

## Planned

- Local agent process orchestration (spawn, monitor, stream logs, kill)
- MCP server configuration editing
- Activity log with real-time agent output
- System tray for background agent management
- Cross-platform MCP config path support
