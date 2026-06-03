# Automatic CLI

The `automatic` command-line interface ships inside the desktop app's binary.
It exposes a read-mostly subset of the configuration surface so you can list,
inspect, and sync projects from any shell, plus the bare minimum of mutations
needed for everyday work (memory writes and project sync).

The same Rust crate hosts the GUI, the MCP stdio server, and the CLI; all three
call the same `core::*` functions, so behaviour and data model are identical.

## Installing

Open the desktop app, navigate to **Settings → Command Line**, and click
**Install**. This creates a symlink from the bundled binary to a directory on
your `$PATH`:

| Platform | Preferred path | Fallback / extra |
| --- | --- | --- |
| macOS, Linux | `/usr/local/bin/automatic` (if writable) | `~/.local/bin/automatic` |
| Windows | `%LOCALAPPDATA%\Programs\automatic\bin\automatic.exe` (no admin needed) | dir is prepended to `HKCU\Environment\Path` |

If the macOS / Linux fallback `~/.local/bin` is not already on your `$PATH`,
the settings page surfaces a hint with the shell snippet to add.

On Windows, the install button copies the bundled `automatic-cli.exe`
console binary into the per-user bin directory and broadcasts
`WM_SETTINGCHANGE` so already-running shells refresh their `PATH`. Newly
opened terminals always see the change.

### Windows: two binaries, on purpose

The Windows GUI binary (`Automatic.exe`) is linked as a windows-subsystem
app so launching it from Explorer does not flash a console. That same
flag stops `cmd.exe` and PowerShell from waiting for it, which makes the
GUI binary unsuitable for direct CLI invocation: stdout would flush too
late and exit codes would not propagate.

The `automatic-cli.exe` binary built from `src/bin/automatic-cli.rs` is a
console-subsystem app that forwards directly to `automatic_lib::cli::run`.
The install action drops a copy of this binary on `PATH` so `automatic
<verb>` works end-to-end from any shell. Both binaries share the same Rust
crate and the same `cli::run` entry point, so behaviour matches the
single-binary install on macOS / Linux exactly.

## Global flags

| Flag | Effect |
| --- | --- |
| `--json` | Emit machine-readable JSON. Shapes match the MCP server's responses. |
| `--quiet` | Suppress non-essential human output. No effect with `--json`. |
| `--help`, `-h` | Show help for the current command. |
| `--version`, `-V` | Print the Automatic version. |

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Requested entry not found (project, skill, rule, memory key) |
| 2 | Usage error (bad flags or missing arguments) |
| 3 | I/O or configuration error |

## Commands

### Projects

```
automatic projects list
automatic projects show <name>
automatic projects sync <name>
```

`show` prints the full project config (registry plus the on-disk copy under
the project directory). `sync` writes agent config files into the project
directory and returns the list of files written.

### Skills

```
automatic skills list
automatic skills show <name>
automatic skills search <query>
```

`list` enumerates every skill discovered across the managed library and any
read-only external sources (e.g. `~/.agents/skills`, `~/.claude/skills`).
`show` prints the raw `SKILL.md` content. `search` filters by case-insensitive
substring on the skill name.

### MCP servers

```
automatic mcp list
```

Lists MCP server configs registered with Automatic. Use the GUI to add, edit,
or remove servers — the CLI is read-only here in v1.

### Memory

```
automatic memory list <project> [pattern]
automatic memory get <project> <key>
automatic memory set <project> <key> <value> [--source <tag>]
automatic memory search <project> <query>
```

`list` accepts an optional case-insensitive substring filter. `set` defaults
the `source` tag to `cli` so memories written from the terminal are
distinguishable from GUI- or agent-authored entries.

### Rules

```
automatic rules list
automatic rules show <machine-name>
```

### Init — apply a template to a directory

```
automatic init <template> [--directory <path>] [--name <name>]
```

Applies a project template to a directory and writes the same files
`automatic projects sync` would write — agent config files, skill copies,
hooks, instruction files, and any inline `project_files` from the template.

Unlike `projects sync`, **no project is created and no registry entry is
written**. The projects registry, the activity log, and the global MCP
server registry are all untouched. Use this when you want template-driven
setup for a directory that is not (and should not become) a tracked
Automatic project.

| Flag | Default | Description |
| --- | --- | --- |
| `--directory <path>` | `$PWD` | Target directory. Must exist. |
| `--name <name>` | directory basename, sanitised | Synthetic project name used internally for filename generation. |

Example:

```bash
mkdir my-app && cd my-app
automatic init software-defaults
# Wrote 7 files into ./ — CLAUDE.md, AGENTS.md, .agents/skills/*, .mcp.json, …
```

The `--json` form returns the list of paths written so a script can
verify the files or copy them elsewhere.

## Scripting

Combining `--json` with a JSON tool such as `jq` covers most automation
needs:

```bash
# Every project name as a newline-separated list
automatic projects list --json | jq -r '.[]'

# Skills that exist in the library but not in any external source
automatic skills list --json \
  | jq -r '.[] | select(.sources == ["library"]) | .name'

# Sync every project, exit non-zero on the first failure
automatic projects list --json \
  | jq -r '.[]' \
  | xargs -I{} automatic projects sync {} --quiet
```

The shapes returned by `--json` are the same Rust structs the MCP server
returns, so a script that targets one surface can move to the other without
reshaping.

## Out of scope in v1

The CLI deliberately does **not** include:

- Skill, rule, or MCP-server mutations (use the GUI).
- Interactive prompts or TTY-aware UI.
- Shell completions (`clap_complete` integration is queued for v2).

### Windows bundle wiring (deployment follow-up)

The Settings install action looks for `automatic-cli.exe` next to the
running GUI binary. For the bundled Windows MSI / NSIS installer to
actually place it there, `src-tauri/tauri.conf.json` needs an
`externalBin` entry pointing at the CLI binary, e.g.:

```jsonc
"bundle": {
  // ...
  "externalBin": ["../target/release/automatic-cli"]
}
```

`cargo tauri build` then expects `automatic-cli-x86_64-pc-windows-msvc.exe`
to exist; CI builds for Windows must build both `[[bin]]` entries
(`cargo build --release --bin automatic --bin automatic-cli`) before
invoking `tauri build`. The macOS / Linux bundle does not need this — the
GUI binary IS the install target on Unix.

This wiring cannot be validated from a macOS host. The install action
will return a descriptive error (`CLI binary not found at ...`) until the
bundle ships the CLI, so users on a pre-bundle build see a clean
failure rather than a partial install.
