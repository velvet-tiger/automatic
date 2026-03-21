# Session Tracking

**Date:** 2026-03-14  
**Status:** Planned (implementation pending)

---

## Overview

Automatic tracks active coding agent sessions and exposes them via:

- The `automatic_list_sessions` MCP tool — consumed by agents to discover what else is running
- The **Agents tab** UI — an "Activity" section on the per-agent detail pane showing sessions for that agent

Three agents support session tracking:

| Agent | ID | Native store |
|---|---|---|
| Claude Code | `claude` | `~/.automatic[-dev]/sessions.json` (written by hook scripts) |
| Codex CLI | `codex` | `~/.codex/session_index.jsonl` + `sessions/YYYY/MM/DD/*.jsonl` |
| OpenCode | `opencode` | `~/.local/share/opencode/storage/session/*/ses_*.json` |

All other agents return an empty list via the default trait implementation.

---

## Architecture Decision: Sessions Belong on the Agent Trait

### Context

The original implementation was Claude Code-only. Session data was written to `~/.automatic/sessions.json` by bash hook scripts (registered via `claude plugin`), and `core/plugins.rs` owned a standalone `list_sessions()` function that read that file directly. There was no abstraction — the function was Claude-specific by design.

When adding Codex CLI and OpenCode support, one approach was to create a new `core/agent_sessions.rs` module with per-agent reader functions (`list_codex_sessions()`, `list_opencode_sessions()`, etc.) called from a central aggregator. This would have worked but placed session-reading logic outside the agents, inconsistent with how every other agent capability (MCP discovery, skill sync, detection) is structured.

### Decision

Session reading is implemented as a method on the `Agent` trait in `src-tauri/src/agent/mod.rs`:

```rust
/// Return active sessions for this agent by reading its native session store.
/// The default implementation returns an empty vec — agents without session
/// tracking do not need to override this.
fn list_sessions(&self) -> Vec<AgentSession> {
    vec![]
}
```

Each agent that supports session tracking implements this method on its own struct. The aggregator is a free function in `agent/mod.rs`:

```rust
pub fn list_all_sessions() -> Vec<AgentSession> {
    all().iter().flat_map(|a| a.list_sessions()).collect()
}
```

`core/plugins.rs` delegates to this function. It no longer contains any agent-specific reading logic.

### Consequences

- Session reading logic for each agent lives in the agent's own file — the same file that owns its config writing, detection, and discovery. No cross-file coupling for agent-specific concerns.
- Adding session tracking to a new agent requires only implementing one trait method. No central module changes.
- Agents without session tracking (11 of 14) require no changes at all — the default no-op satisfies the compiler.
- The old standalone `list_sessions()` in `core/plugins.rs` is replaced by a thin delegate to `agent::list_all_sessions()`.

---

## Architecture Decision: Unified Array Return Shape

### Context

The original `get_sessions` Tauri command and `automatic_list_sessions` MCP tool returned a raw JSON string serialized from `~/.automatic/sessions.json` — a JSON **object** keyed by session ID:

```json
{
  "<session_id>": {
    "session_id": "...",
    "cwd": "...",
    "model": "...",
    "source": "...",
    "started_at": "...",
    "last_seen": "..."
  }
}
```

This shape was convenient for Claude's bash hook (which writes and deletes by key), but not well-suited to multi-agent output: there is no natural key for a merged collection from multiple agents, and there is no discriminator identifying which agent the session belongs to.

### Decision

The return shape is a JSON **array** of `AgentSession` objects, each with an `agent` field:

```json
[
  {
    "agent": "claude",
    "id": "abc123",
    "cwd": "/Users/xtfer/working/myproject",
    "model": "claude-opus-4-6",
    "source": "vscode",
    "title": "",
    "started_at": "2026-03-14T09:12:00Z"
  },
  {
    "agent": "codex",
    "id": "019c9689-22af-7f10-b866-3737541f5f5c",
    "cwd": "/Users/xtfer/working/other",
    "model": "openai",
    "source": "terminal",
    "title": "Standardize colours on automatic web",
    "started_at": "2026-02-25T20:41:35Z"
  }
]
```

### Consequences

- The `get_sessions` Tauri command and `automatic_list_sessions` MCP tool return an array instead of an object. This is a breaking change to the MCP tool shape. The tool description is updated to document the array format.
- The frontend (`Agents.tsx`) was never calling `get_sessions` before this feature was built, so there is no frontend breakage.
- MCP tool consumers (agents) that were reading the old object format will need to handle an array. As of the time of writing, no known agent was reading this tool's output in a way that depended on the object shape.

---

## Native Session Store Formats

### Claude Code

**Path:** `~/.automatic/sessions.json` (or `~/.automatic-dev/sessions.json` in debug builds)

**Written by:** `register-session.sh` — a bash script registered as a Claude Code `SessionStart` hook via the Automatic plugin system. See `docs/plugins.md` for the Claude Code plugin architecture.

**Format:** JSON object keyed by session ID. Each value has:

| Field | Type | Description |
|---|---|---|
| `session_id` | string | Claude Code session UUID |
| `cwd` | string | Working directory at session start |
| `model` | string | Model name (e.g. `"claude-opus-4-6"`) |
| `source` | string | Session source (e.g. `"vscode"`, `"terminal"`) |
| `started_at` | string | ISO8601 UTC timestamp |
| `last_seen` | string | ISO8601 UTC timestamp (updated on each hook fire) |

**Active sessions:** Any entry present in the file. Entries older than 24 hours are pruned by `register-session.sh` on each new session start. Sessions are removed by `deregister-session.sh` on `SessionEnd`.

**Agent impl:** `src-tauri/src/agent/claude_code.rs` — `list_sessions()` reads and deserializes this file.

---

### Codex CLI

**Paths:**
- `~/.codex/session_index.jsonl` — lightweight index of all non-archived sessions (one JSON line per session)
- `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<UUID>.jsonl` — full session event logs, one file per session

**Format of `session_index.jsonl`:** One JSON object per line:

```jsonc
{"id": "<UUID v7>", "thread_name": "Add classic snake game demo", "updated_at": "2026-03-09T12:13:43.053379Z"}
```

**Format of session `.jsonl` files:** Newline-delimited JSON event stream. The **first line** is always a `session_meta` record:

```jsonc
{
  "timestamp": "2026-02-25T20:41:35.663Z",
  "type": "session_meta",
  "payload": {
    "id": "<UUID v7>",
    "timestamp": "2026-02-25T20:41:35.663Z",
    "cwd": "/Users/xtfer/working/myproject",
    "originator": "Codex Desktop",
    "cli_version": "0.105.0-alpha.8",
    "source": "vscode",
    "model_provider": "openai"
  }
}
```

**Active sessions:** A session is active if it appears in `session_index.jsonl`. Sessions moved to `~/.codex/archived_sessions/` are not present in the index.

**Reading strategy:** The agent reads `session_index.jsonl` to get the set of active IDs and their thread names. For each ID, it locates the corresponding `.jsonl` file in the `sessions/YYYY/MM/DD/` tree by filename pattern (`rollout-*-<id>.jsonl`), reads only the first line, and extracts `cwd`, `source`, `model_provider`, and `timestamp` from the `session_meta` payload. Files not found on disk are silently skipped.

**Agent impl:** `src-tauri/src/agent/codex_cli.rs` — `list_sessions()`.

---

### OpenCode

**Path:** `~/.local/share/opencode/storage/session/`

**Structure:** Sessions are organized by project ID:

```
storage/session/
├── <project-uuid>/
│   └── ses_<id>.json
├── <project-uuid>/
│   └── ses_<id>.json
└── global/
    └── ses_<id>.json
```

**Format of `ses_<id>.json`:**

```jsonc
{
  "id": "ses_3fd4d826dffe3MfprSwuBf6N5C",
  "slug": "misty-moon",
  "version": "1.1.36",
  "projectID": "<project-uuid>",
  "directory": "/Users/xtfer/working/myproject",
  "title": "Fix button styles",
  "time": {
    "created": 1769571777938,
    "updated": 1769571831250
    // "archived": 1769650766825  -- only present on archived sessions
  },
  "summary": {
    "additions": 47,
    "deletions": 46,
    "files": 2
  }
  // "parentID": "ses_..."  -- only present on sub-agent sessions
}
```

**Active sessions:** A session file where `time.archived` is **absent**. Sessions with `time.archived` set have been explicitly archived by the user in the OpenCode UI.

**Sub-agent sessions:** OpenCode spawns sub-agents (e.g. `@explore`) as child sessions with a `parentID` field. These are excluded from the session list — only top-level sessions are returned.

**`model` field:** OpenCode does not store the model name at the session level (it is stored per-message). The `model` field in `AgentSession` is left empty for OpenCode sessions.

**Agent impl:** `src-tauri/src/agent/opencode.rs` — `list_sessions()` walks all `*/ses_*.json` files and applies the above filters.

---

## `AgentSession` Struct

Defined in `src-tauri/src/agent/mod.rs`. Serialized to JSON for both the Tauri command and the MCP tool.

```rust
#[derive(Debug, Clone, Serialize)]
pub struct AgentSession {
    pub agent: String,      // agent id: "claude" | "codex" | "opencode"
    pub id: String,         // session identifier (format varies by agent)
    pub cwd: String,        // working directory at session start
    pub model: String,      // model name or provider; empty string for OpenCode
    pub source: String,     // launch context: "vscode", "terminal", etc.; empty if unknown
    pub title: String,      // human-readable session title; empty for Claude Code
    pub started_at: String, // ISO8601 UTC string
}
```

Field availability by agent:

| Field | Claude Code | Codex CLI | OpenCode |
|---|---|---|---|
| `agent` | `"claude"` | `"codex"` | `"opencode"` |
| `id` | Claude session UUID | UUID v7 | `ses_` prefixed ID |
| `cwd` | from hook payload | from `session_meta` | `directory` field |
| `model` | model name | `model_provider` string | `""` (not stored at session level) |
| `source` | from hook payload | from `session_meta` | `""` (not stored at session level) |
| `title` | `""` | `thread_name` from index | `title` field |
| `started_at` | `started_at` from hook | `timestamp` from `session_meta` | `time.created` as ISO8601 |

---

## File Layout

```
src-tauri/src/
├── agent/
│   ├── mod.rs              # AgentSession struct, list_sessions() trait method (default: vec![]),
│   │                       # list_all_sessions() aggregator free function
│   ├── claude_code.rs      # list_sessions() — reads ~/.automatic/sessions.json
│   ├── codex_cli.rs        # list_sessions() — reads ~/.codex/session_index.jsonl + JSONL tree
│   └── opencode.rs         # list_sessions() — walks ~/.local/share/opencode/storage/session/
└── core/
    └── plugins.rs          # list_sessions() delegates to agent::list_all_sessions(),
                            #   serializes Vec<AgentSession> to JSON array string

src/
└── Agents.tsx              # "Activity" section in agent detail pane; sidebar dot indicator
```

---

## How to Add Session Tracking to a New Agent

### 1. Identify the native session store

Determine where the agent stores session state on disk. You need to answer:
- What directory/files constitute the session store?
- How is "active" distinguished from "completed" or "archived"?
- What fields are available? Minimum requirement: `id`, `cwd`, `started_at`.

### 2. Implement `list_sessions()` on the agent struct

In the agent's file (e.g. `src-tauri/src/agent/my_agent.rs`), add to the `impl Agent for MyAgent` block:

```rust
fn list_sessions(&self) -> Vec<super::AgentSession> {
    let Some(home) = super::home_dir() else {
        return vec![];
    };

    let store_path = home.join(".myagent").join("sessions.json");
    let content = match std::fs::read_to_string(&store_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    // Parse and map to AgentSession. Return vec![] on any parse error —
    // session tracking is best-effort and must never panic.
    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    // Map your native format to AgentSession...
    vec![super::AgentSession {
        agent: self.id().to_string(),
        id: "...".to_string(),
        cwd: "...".to_string(),
        model: "...".to_string(),
        source: "...".to_string(),
        title: "...".to_string(),
        started_at: "...".to_string(),
    }]
}
```

Key implementation rules:
- **Return `vec![]` on any error** — never propagate errors from session reading. It is best-effort.
- **Read only what you need** — for large session log files, read only the first line or index file, not the full event stream.
- **Filter out sub-sessions** — if the agent has sub-agent or child session concepts, exclude them.
- **No `unwrap()` or `expect()`** — session reading runs on every `list_sessions` MCP tool call.

### 3. No other changes required

`agent::list_all_sessions()` calls `.list_sessions()` on every registered agent. The new implementation is picked up automatically. No changes to `core/plugins.rs`, `commands/misc.rs`, `mcp.rs`, or `agent/mod.rs` are needed.

### 4. Verify

```bash
# Rust — confirm it compiles and the trait is satisfied
cd src-tauri && cargo check

# Run unit tests
cd src-tauri && cargo test
```

---

## UI: Agents Tab Activity Section

Sessions are displayed in the **Agents tab** under each agent's detail pane.

### Activity section

Located between **Capabilities** and **Default Options** in the agent detail pane (`src/Agents.tsx`). Only rendered for agents that support session tracking (`claude`, `codex`, `opencode`).

- **Section header:** "Activity" with a small pulsing green dot when one or more sessions are active.
- **Per-session row:** Working directory (monospace, truncated), session title (if non-empty — Codex and OpenCode provide these), relative start time ("2 min ago", "1h ago"), and a `source` badge (`vscode`, `terminal`, etc.) when present.
- **Empty state:** "No active sessions" in muted text.

### Sidebar indicator

Agent rows in the left sidebar show a small green dot when the agent has at least one active session.

### Data loading

Sessions are loaded via `invoke("get_sessions")` in a `loadSessions()` function called from the component's `useEffect` alongside the existing `loadAgents()` call. Sessions are stored in a `sessions` state variable typed as `AgentSession[]`. No polling — sessions are loaded once on mount and when the component re-renders.

### TypeScript interface

```typescript
interface AgentSession {
  agent: string;       // "claude" | "codex" | "opencode"
  id: string;
  cwd: string;
  model: string;
  source: string;
  title: string;
  started_at: string;  // ISO8601 UTC string
}
```

Sessions for the currently selected agent are filtered from the full list: `sessions.filter(s => s.agent === selected.id)`.
