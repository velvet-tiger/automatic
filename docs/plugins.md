# Plugin System

**Date:** 2026-03-12  
**Status:** Active

---

## Overview

Automatic supports bundled plugins that contribute tool definitions and UI panels to the application. A plugin is a self-contained unit consisting of:

- A **manifest** declared in `core/app_plugins.rs` — the plugin's identity, description, and tool declaration.
- A **Rust backend** in `src-tauri/src/plugins/<name>/` — business logic and command dispatch.
- A **React frontend** in `src/plugins/<name>/` — the panel rendered in the Tools tab.

Plugins are enabled or disabled by the user from Settings > Plugins. Enabling a plugin registers its tool in `~/.automatic/tools/`. Disabling removes it.

---

## Architecture Decision: Generic Command Dispatch

### Context

Each plugin needs to expose commands to the frontend (e.g. "list features", "get status"). Tauri commands are registered by name in `generate_handler![]` in `lib.rs`. The naive approach registers each plugin command directly:

```rust
// naive — every plugin adds names to lib.rs
tauri::generate_handler![list_spec_kitty_features, get_spec_kitty_status, ...]
```

This was the initial implementation. It had two problems:

1. Every new plugin command required editing `lib.rs` and `commands/mod.rs` — two files with no plugin-specific business and no natural reason to change.
2. Tauri's `generate_handler![]` is a proc-macro. It cannot compose with nested macros, and the `__cmd__` helper macros it generates internally cannot be imported by absolute path (Rust issue [#52234](https://github.com/rust-lang/rust/issues/52234)). There is no way to make `lib.rs` free of plugin command names while still using per-command Tauri registration.

### Decision

All plugin commands flow through a single generic Tauri command, `invoke_tool_command`, defined in `commands/tools.rs`:

```rust
#[tauri::command]
pub fn invoke_tool_command(
    tool: String,
    command: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String>
```

`lib.rs` registers this one command once. When a new plugin is added, `lib.rs` does not change.

The dispatch table in `commands/tools.rs` maps tool names to plugin dispatch functions:

```rust
match tool.as_str() {
    "spec-kitty" => crate::plugins::spec_kitty::dispatch(&command, payload),
    other => Err(format!("Unknown tool: '{}'", other)),
}
```

Each plugin implements a `pub fn dispatch(command: &str, payload: serde_json::Value) -> Result<serde_json::Value, String>` that handles its own command routing and type conversion internally.

### Consequences

- `lib.rs` and `commands/mod.rs` never change when a plugin is added. Only `commands/tools.rs` (one line in the match) and the plugin folder itself change.
- Plugin commands lose Tauri's compile-time typed argument checking. Type safety is enforced at the plugin's `dispatch` boundary instead, with `serde_json::Value` crossing the plugin boundary.
- The frontend calls one generic command regardless of which plugin it is talking to, with a consistent `{ tool, command, payload }` shape.

### Rejected alternatives

**Per-plugin Tauri commands with `pub use` re-exports** — `commands/mod.rs` re-exported plugin commands, and `lib.rs` listed them in `generate_handler![]`. This was an improvement over direct naming in `lib.rs` (plugin commands came into scope via `use commands::*` rather than being named directly), but `lib.rs` still had to list each command by name. Rejected because it still couples `lib.rs` to plugin internals.

**Moving `generate_handler![]` into `commands/mod.rs`** — Attempted. Fails because `generate_handler![]` needs `__cmd__<name>` macros in scope, and macros expanded by `#[tauri::command]` in a submodule cannot be referenced by absolute path from a different module (Rust issue #52234).

---

## File Layout

```
src-tauri/src/
├── core/
│   ├── app_plugins.rs          # Plugin manifests + PluginToolDeclaration
│   └── tools.rs                # ToolDefinition, detection logic (generic)
├── commands/
│   └── tools.rs                # invoke_tool_command + dispatch table
└── plugins/
    └── <plugin-name>/
        ├── mod.rs              # pub use commands::*
        └── commands.rs         # dispatch() + business logic

src/
└── plugins/
    └── <plugin-name>/
        └── <PluginPanel>.tsx   # React panel rendered in the Tools tab
```

The dispatch table in `commands/tools.rs` is the only file outside the plugin folder that names a plugin.

`Projects.tsx` contains the frontend dispatch — a `switch` on tool name that renders the correct panel component. This is the frontend equivalent of `commands/tools.rs`.

---

## How to Add a New Plugin

### 1. Create the Rust plugin

Create `src-tauri/src/plugins/my_tool/commands.rs` with the business logic and a `dispatch` entry point:

```rust
use serde::{Deserialize, Serialize};

// Types returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyToolResult {
    pub name: String,
}

// Plugin dispatch entry point — called by invoke_tool_command
pub fn dispatch(command: &str, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    match command {
        "list_items" => {
            let project_dir: String = payload
                .get("projectDir")
                .and_then(|v| v.as_str())
                .ok_or("missing field: projectDir")?
                .to_string();
            let result = list_items(project_dir)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        other => Err(format!("Unknown my-tool command: '{}'", other)),
    }
}

fn list_items(project_dir: String) -> Result<Vec<MyToolResult>, String> {
    // business logic here
    Ok(vec![])
}
```

Create `src-tauri/src/plugins/my_tool/mod.rs`. The plugin owns its manifest — all identity data lives here, not in core:

```rust
mod commands;
pub use commands::*;

use crate::core::{PluginCategory, PluginManifest, PluginToolDeclaration};
use crate::core::tools::ToolKind;

/// The manifest that describes this plugin to the Automatic registry.
/// Called by `core::app_plugins::bundled_plugins()`.
pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "my-tool".to_string(),
        name: "My Tool".to_string(),
        description: "What this tool does.".to_string(),
        version: "1.0.0".to_string(),
        category: PluginCategory::Integrations,
        enabled_by_default: false,
        tool: Some(PluginToolDeclaration {
            name: "my-tool".to_string(),
            display_name: "My Tool".to_string(),
            description: "Short description shown in the Tools tab.".to_string(),
            url: "https://github.com/example/my-tool".to_string(),
            github_repo: Some("example/my-tool".to_string()),
            kind: ToolKind::Cli,
            detect_binary: Some("my-tool".to_string()),   // optional
            detect_dir: Some(".my-tool-data".to_string()), // optional; takes precedence over detect_binary
        }),
    }
}
```

Use `detect_dir` when the tool initialises a directory inside the project (e.g. `.my-tool-data/`). This prevents false positives from version manager shims on `$PATH`. See `core/tools.rs` for the detection precedence rules.

Register the module in `src-tauri/src/plugins/mod.rs`:

```rust
pub mod spec_kitty;
pub mod my_tool;    // add this
```

### 2. Register in the two dispatch tables

**Backend** — in `src-tauri/src/commands/tools.rs`, add one line to `invoke_tool_command`:

```rust
match tool.as_str() {
    "spec-kitty" => crate::plugins::spec_kitty::dispatch(&command, payload),
    "my-tool"    => crate::plugins::my_tool::dispatch(&command, payload),   // add this
    other => Err(format!("Unknown tool: '{}'", other)),
}
```

**Plugin registry** — in `src-tauri/src/core/app_plugins.rs`, add one line to `bundled_plugins()`:

```rust
fn bundled_plugins() -> Vec<PluginManifest> {
    vec![
        crate::plugins::spec_kitty::manifest(),
        crate::plugins::my_tool::manifest(),    // add this
    ]
}
```

These are the only two changes required outside the plugin folder.

### 4. Create the React panel

Create `src/plugins/my-tool/MyToolPanel.tsx`:

```tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface MyToolResult {
  name: string;
}

interface Props {
  projectDir: string;
  sidebar?: React.ReactNode;
}

export function MyToolPanel({ projectDir, sidebar }: Props) {
  const [items, setItems] = useState<MyToolResult[]>([]);

  useEffect(() => {
    invoke<MyToolResult[]>("invoke_tool_command", {
      tool: "my-tool",
      command: "list_items",
      payload: { projectDir },
    }).then(setItems).catch(console.error);
  }, [projectDir]);

  return (
    <div className="flex gap-6 items-start">
      <section className="flex-1 min-w-0">
        {items.map((item) => (
          <div key={item.name}>{item.name}</div>
        ))}
      </section>
      {sidebar && <aside>{sidebar}</aside>}
    </div>
  );
}
```

### 5. Wire the panel into Projects.tsx

In `src/Projects.tsx`, add the import at the top:

```tsx
import { MyToolPanel } from "./plugins/my-tool/MyToolPanel";
```

In the `ProjectToolDetailPanel` render function, add a branch to the tool name switch:

```tsx
if (entry.name === "my-tool") {
  return (
    <MyToolPanel
      projectDir={project.directory ?? ""}
      sidebar={<ToolInfoSidebar entry={entry} />}
    />
  );
}
```

### 6. Verify

```bash
# Rust
cd src-tauri && cargo check

# Frontend
npm run build
```

---

## Detection: detect_dir vs detect_binary

`ToolDefinition` supports two detection signals:

| Field | What it checks | When to use |
|-------|---------------|-------------|
| `detect_dir` | `<project_dir>/<value>` exists on disk | The tool initialises a directory inside the project (e.g. `kitty-specs/`, `.my-tool/`) |
| `detect_binary` | `which <value>` finds the binary on `$PATH` | The tool has no project-level directory marker |

**`detect_dir` takes precedence.** When both are set, only `detect_dir` is evaluated for project-level detection. This prevents version manager shims (pyenv, rbenv, etc.) from triggering false positives: a shim on `$PATH` does not mean the tool has been initialised in this project.

When neither field is set, the tool is never auto-detected.

---

## Existing Plugins

### Spec Kitty

| Field | Value |
|-------|-------|
| Plugin ID | `spec-kitty` |
| Tool name | `spec-kitty` |
| detect_dir | `kitty-specs` |
| detect_binary | `spec-kitty` |
| Category | Integrations |
| Enabled by default | No |

Spec Kitty is a spec-driven development CLI for AI agents. It stores feature specifications, plans, and work package kanban state under `kitty-specs/<slug>/` in the project directory.

**Backend commands** (via `invoke_tool_command` with `tool: "spec-kitty"`):

| command | payload fields | returns |
|---------|---------------|---------|
| `list_features` | `projectDir: string` | `SpecKittyFeatureMeta[]` |
| `get_status` | `projectDir: string`, `featureSlug: string` | `SpecKittyFeatureStatus` |

`list_features` reads `meta.json` from each subdirectory of `kitty-specs/`. `get_status` shells out to `spec-kitty agent tasks status --feature <slug> --json` — work package lane state is not stored on disk and can only be retrieved via the CLI.

**Frontend:** `src/plugins/spec-kitty/SpecKittyPanel.tsx` — renders a features list with expandable per-feature WP kanban.
