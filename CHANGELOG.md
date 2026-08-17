# Changelog

All notable changes to Automatic are documented here.

## [Unreleased]

### Added

- New agent tool: Z Code (Z.ai's desktop agentic development environment, zcode.z.ai). Syncs project instructions to the shared `AGENTS.md`, skills to `.zcode/skills/`, and MCP servers merged into the shared `.zcode/config.json` under `mcpServers` — the user's other config keys are preserved on both sync and removal. Hooks, commands, and sub-agents are deliberately not synced: Z Code ignores project-level hooks for security, and its workspace-level command/sub-agent paths are undocumented (sub-agents are beta at workspace scope).

## [1.19.0] - 2026-08-16

### Added

- Rules panel now has a Globe button on each project (custom) rule that promotes it into the reusable rule library. The rule is saved to the library, removed from the project's inline custom rules, and re-added by machine name to the project's global rules so behaviour is preserved. The machine name is slugified from the display name and disambiguated on collision. ([4e90a07](https://github.com/velvet-tiger/automatic/commit/4e90a07))
- Skills page now uses a full-width sortable table with a right-side drawer editor, replacing the narrow sidebar + preview layout. Adds per-row and select-all checkboxes plus a "Delete selected" action bar. Bundled skills are un-deletable at both the UI (checkbox suppressed, Lock icon shown) and the backend (`delete_skill` refuses them via the new `is_bundled_skill` guard), matching how the built-in `automatic` skill and plugin-provided skills are already protected. ([a0e6a48](https://github.com/velvet-tiger/automatic/commit/a0e6a48))

## [1.18.1] - 2026-08-14

### Fixed

- Toggling a plugin in Settings → Plugins now updates the sidebar and Tools-home cards immediately. `usePlugin` previously read plugin state once at mount with no way to react to later changes, so a newly enabled plugin (e.g. Maildev) required a full app restart before its nav item would appear. ([9dee61f](https://github.com/velvet-tiger/automatic/commit/9dee61f))
- Plugins can now declare a tool as `project_scoped: false` for machine-wide features with no per-project effect. The Maildev tool is marked this way, so it no longer appears as an addable entry in a project's Tools tab, and neither auto-detect path adds it to a project just because the `maildev` binary is on `$PATH`. ([9dee61f](https://github.com/velvet-tiger/automatic/commit/9dee61f))

## [1.18.0] - 2026-08-13

### Added

- New Maildev plugin: detects the `maildev` CLI, provides an on/off toggle that always runs it with `--mcp`, links to its admin UI, and registers its MCP server in the Library. Adds a new `mcp_servers` declaration kind to `PluginManifest`, modeled on the existing rules sync: installed on enable, left in place on disable since a project may already reference it. ([84a1dfc](https://github.com/velvet-tiger/automatic/commit/84a1dfc))
- The Dev Servers plugin now detects and links dev server URLs from process output. Dev servers print their listen URL on startup (Vite's "Local: http://…", Next.js's "- Local: …", etc); Automatic parses stdout/stderr for local/private URLs as they stream in and surfaces them as clickable Open links in the Servers panel, instead of relying only on a manually-entered port. ([0878610](https://github.com/velvet-tiger/automatic/commit/0878610))

## [1.17.1] - 2026-08-12

### Fixed

- The app now resolves the user's real `PATH` from their login shell at startup, instead of inheriting the minimal `PATH` Finder/Dock-launched apps get on macOS and Linux. This fixed the Dev Servers plugin reporting `'npm' was not found on $PATH` even when npm was installed (via nvm, Homebrew, etc.) and working fine from a terminal; the same fix applies to editor CLI detection and MCP command detection. ([12f0cb3](https://github.com/velvet-tiger/automatic/commit/12f0cb3))

## [1.17.0] - 2026-08-12

### Added

- New Dev Servers plugin: start, stop, and monitor npm, pnpm, and yarn dev servers for a project. Adds a "Servers" tab to each project (list, start/stop, live output log, create/edit) and a cross-project "Servers" view under the global Tools section, both gated on the plugin being enabled. Stopping a server kills the whole process tree it spawned, not just the package-manager wrapper. ([37b58de](https://github.com/velvet-tiger/automatic/commit/37b58de))

## [1.16.0] - 2026-08-11

### Added

- Antigravity now discovers global MCP servers from the config file it shares with the Gemini CLI (`~/.gemini/config/mcp_config.json`), and Codex CLI marks a remote MCP server configured for OAuth with `auth = "oauth"` so Codex's own OAuth flow picks it up. ([6452415](https://github.com/velvet-tiger/automatic/commit/6452415))
- Zed and Warp now migrate their legacy instruction files (`.rules` and `WARP.md`) to `AGENTS.md` on sync, following the same rules Cursor already used for its `.cursorrules` migration. Where a legacy file can't be safely merged, sync now surfaces it as an instruction conflict instead of leaving the vendor silently reading stale content forever. ([ae5d61e](https://github.com/velvet-tiger/automatic/commit/ae5d61e))
- Added agent support for GitHub Copilot, Droid, and Kiro, including Kiro's newly rebuilt custom-agent JSON format. ([fb66063](https://github.com/velvet-tiger/automatic/commit/fb66063))
- Hook configuration now participates in drift detection for every hook-capable agent except Cursor, so a project's hooks no longer silently drift from what's configured with no signal in the UI. ([3db53f0](https://github.com/velvet-tiger/automatic/commit/3db53f0))
- Gemini CLI, GitHub Copilot, and Droid now support lifecycle hooks, matching capabilities each vendor already shipped: Gemini CLI merges hooks into `.gemini/settings.json`, Copilot writes `.github/hooks/automatic.json`, and Droid owns `.factory/hooks.json`. ([2a5362f](https://github.com/velvet-tiger/automatic/commit/2a5362f))
- Codex CLI's and Claude Code's hook event lists are now sourced from Rust instead of hand-maintained frontend arrays, closing gaps where Codex was missing 5 of the 11 documented events and Claude Code was missing `MessageDisplay`. ([1ffc693](https://github.com/velvet-tiger/automatic/commit/1ffc693))
- Kilo Code (rebranded to Kilo) now syncs to its new OpenCode-derived config format (`kilo.json` / `.kilo/kilo.json`, with MCP servers under a top-level `mcp` key) instead of the old `.kilocode/mcp.json` shape Kilo no longer reads, with a one-time migration that removes the legacy directory once its servers are already selected for the project. ([0fb0b14](https://github.com/velvet-tiger/automatic/commit/0fb0b14))

### Fixed

- Sub-agent stale-file cleanup was silently broken for any vendor using a compound file extension, such as GitHub Copilot's `{name}.agent.md`: the machine name recovered from the filename was never valid, so an orphaned sub-agent file could never be recognised or swept. Cleanup also previously deleted any file matching a vendor's extension with no ownership check at all. ([fb66063](https://github.com/velvet-tiger/automatic/commit/fb66063))
- Importing a skill from a repository that is just a flat collection of `skills/<name>/SKILL.md` directories with no manifest (for example `evilmartians/agent-skills`) now imports every skill found, instead of importing nothing. ([b8be6ad](https://github.com/velvet-tiger/automatic/commit/b8be6ad))
- Zed no longer writes dead sub-agent files into `.zed/agents/`, a directory Zed never reads. ([89d4c7e](https://github.com/velvet-tiger/automatic/commit/89d4c7e))
- MCP drift detection for the agents that merge into an existing config file (Codex CLI, Gemini CLI, GitHub Copilot, OpenCode, Zed) no longer reports permanent, unclearable drift. The check was comparing against an empty file instead of the agent's real config. ([eaac483](https://github.com/velvet-tiger/automatic/commit/eaac483))
- Gemini CLI, Zed, and GitHub Copilot no longer wipe their shared config file (`.gemini/settings.json`, `.zed/settings.json`, `.vscode/mcp.json`) on a single JSON syntax error. A parse failure now raises an error instead of silently falling back to an empty file and discarding the user's own settings. Warp also no longer deletes `AGENTS.md` when removed from a project, since it doesn't own that shared file. ([8a67679](https://github.com/velvet-tiger/automatic/commit/8a67679))
- `tauri dev` builds no longer trigger a fresh macOS keychain confirmation prompt on every rebuild. Dev binaries are now signed with a stable self-signed identity via a cargo runner instead of an ad-hoc signature that changes on every build; redundant keychain reads were also removed. ([38091a6](https://github.com/velvet-tiger/automatic/commit/38091a6))
- Junie's `.junie/skills` directory is now covered by drift detection. Its hand-written sync override only ever wrote `.agents/skills`, so it sat outside drift checks entirely; all 16 per-agent overrides are now derived from `skill_dirs()` so the two can no longer disagree. ([8deb9cb](https://github.com/velvet-tiger/automatic/commit/8deb9cb))
- The bundled `automatic-service` rule now documents the project's full MCP tool surface (rules and hooks CRUD, full memory management, credentials, sessions, project context) and updates automatically on existing installs, instead of only being written once and then staying stale. ([346ff8c](https://github.com/velvet-tiger/automatic/commit/346ff8c))
- Junie's MCP config path and instructions file path are updated to match JetBrains' current layout: MCP config now reads from `.junie/mcp/mcp.json`, and `.junie/AGENTS.md` is treated as canonical with `.junie/guidelines.md` as a legacy fallback. ([c2ce520](https://github.com/velvet-tiger/automatic/commit/c2ce520))

### Changed

- Claude Code's and Codex CLI's hook-writing logic is now shared through a common `HookWriteSpec`, replacing two independent ~250-line implementations of the same grouping, merging, and cleanup behaviour. ([d4d0ace](https://github.com/velvet-tiger/automatic/commit/d4d0ace))
- OpenCode's MCP config writer is now a shared helper (`write_opencode_dialect_mcp_config`) that any OpenCode-dialect vendor can target at a different file, rather than logic embedded only in OpenCode's own writer. ([d37bb5d](https://github.com/velvet-tiger/automatic/commit/d37bb5d))

### Documentation

- Added an audit of agent configuration sync gaps across all 16 supported agents and the phased remediation plan that closed them across this release. ([deaca44](https://github.com/velvet-tiger/automatic/commit/deaca44))

### Testing

- Feature-tracking tests now run against a per-test temporary database instead of sharing one, fixing an intermittent failure where one test's cleanup could delete rows another parallel test was still using, and stopping the test suite from writing into the developer's own `~/.automatic-dev`. ([92efb40](https://github.com/velvet-tiger/automatic/commit/92efb40))
- Added contract tests that check every agent's declared capabilities against what its sync code actually does, catching cases where a capability flag and its implementation disagree (for example Zed previously claiming sub-agent support it didn't have). ([55300b9](https://github.com/velvet-tiger/automatic/commit/55300b9))

### Maintenance

- Updated this project's own `AGENTS.md` and docs index to document the full Automatic MCP tool surface (rules, hooks, features, credentials, sessions) and link the new agent audit and remediation-plan documents. ([b92d6b1](https://github.com/velvet-tiger/automatic/commit/b92d6b1))

## [1.15.2] - 2026-07-28

### Fixed

- Library skills still overwrite on sync, but project-scoped custom skills, rules, agents, and commands now favour on-disk content when they diverge from the stored snapshot. Sync surfaces an adopt/overwrite comparison instead of silently replacing the user's edits. ([0bdb84f](https://github.com/velvet-tiger/automatic/commit/0bdb84f))
- Updating or reinstalling built-in skills no longer leaves project copies stale. Projects are resynced after bundled skill reinstalls (including the upgrade-time path), so drift does not fire immediately after an update. ([d1d6d71](https://github.com/velvet-tiger/automatic/commit/d1d6d71))
- Add Project now refuses to create a project when the name or directory already exists. The wizard stub save was overwriting existing project configs; duplicate creates surface an error instead of wiping data. ([0462561](https://github.com/velvet-tiger/automatic/commit/0462561))
- Creating a feature from the Build UI now lands in the selected column. The UI already sent state on create, but the backend always inserted backlog; an optional state is accepted through the command and MCP paths. ([0a239a6](https://github.com/velvet-tiger/automatic/commit/0a239a6))
- Uninstalling or dropping a bundled plugin now removes its orphaned tool file and `project.tools` references (for example Spec Kitty leftovers). Cleanup runs during plugin tool sync on startup and on plugin toggle. ([23b2f7b](https://github.com/velvet-tiger/automatic/commit/23b2f7b))
- Clearing assignee or effort in the Build list now persists. Serde treated JSON null as "leave unchanged" on nested Option fields, so those clears were a no-op. ([64d151f](https://github.com/velvet-tiger/automatic/commit/64d151f))

## [1.15.1] - 2026-07-26

### Fixed

- Cursor and Codex MCP configs are now written in each agent's own dialect instead of a Claude Code schema with minor tweaks. Cursor gets `${env:VAR}` placeholders and an `auth` block; Codex gets transport inferred from `command`/`url` (no `type`), Authorization via the fields it actually reads, and `cwd` preserved. Inherited env is resolved per agent at write time so sync and drift always agree on the expected file. ([bdc8bac](https://github.com/velvet-tiger/automatic/commit/bdc8bac))
- Applying a project template now includes its rules. Rules were previously diverted into a pending side-channel that the Rules tab never showed, and rules-only templates (or the CLI init path) could discard them entirely. Pending instruction content is also scoped to its owning project so an unsaved apply cannot write one project's instruction into another. ([daf7fee](https://github.com/velvet-tiger/automatic/commit/daf7fee))

## [1.15.0] - 2026-07-23

### Added

- Cursor support has been modernized to match current Cursor, and the "(Beta)" label is gone. Instructions now sync to `AGENTS.md` instead of the legacy `.cursorrules`, with a one-time migration that folds user content across (on conflict the legacy file is kept with managed sections stripped for manual review). Hooks now sync to `.cursor/hooks.json`, with ownership tracked in a sidecar manifest so the vendor file stays schema-clean and user-authored or user-edited handlers always survive; the Hooks page gains Cursor's event catalogue. Commands sync to `.cursor/commands/`. A new per-project agent option (default off) additionally writes library rules as native `.cursor/rules/*.mdc` files with frontmatter, mirroring the `.claude/rules/` pattern for drift, rename/delete propagation, and cleanup. Stdio MCP entries now carry an explicit `"type": "stdio"` per Cursor's docs. ([7f52818](https://github.com/velvet-tiger/automatic/commit/7f52818))

### Fixed

- A stale plugin-level `.mcp.json`, written into the plugin marketplace source by an old build running quarantined (App-Translocated) and pointing at an ephemeral `/private/var/.../AppTranslocation/...` binary path, kept resurfacing in agent plugin caches on every reinstall and made agents spawn a dead path. The stale file is now actively deleted when the plugin is written, and Automatic refuses to emit a translocated binary path, preferring the live CLI symlink. ([14b5d08](https://github.com/velvet-tiger/automatic/commit/14b5d08))
- Alternating launches through the CLI symlink and the app bundle no longer trigger resync storms. macOS reports the invocation path, so the same binary appeared as two different strings and every `mcp-serve` spawn flagged a "binary change" — rewriting `.mcp.json`, skills, and `.automatic` content across all projects every few seconds and flashing Vite reloads in projects watching those files. Binary paths are now resolved once, canonically, and compared by target identity so an alias is never a change but a genuinely stale path still triggers repair. ([0f91c48](https://github.com/velvet-tiger/automatic/commit/0f91c48))
- A project could show duplicate case-variant rows for one MCP server (for example `Sentry` and `sentry`): the global registry is case-insensitive on disk, but project membership checks were exact-match, so Add-from-Library and autodetect could each land their own casing. Project MCP lists are now de-duplicated case-insensitively on save and read, discovery uses a case-insensitive membership check, and a case-only change is no longer mis-logged as a remove + add. ([ff13b1c](https://github.com/velvet-tiger/automatic/commit/ff13b1c))

## [1.14.3] - 2026-07-22

### Fixed

- Remote HTTP/SSE MCP servers authenticated with OAuth no longer silently revert to a broken "local" server on sync. Such servers are stored in project files as a local `mcp-proxy` stub (to keep the token in the keychain); a later autodetect-sync would discover that stub and write it back over the authoritative registry entry, dropping the url/oauth/enabled fields and breaking the proxy — typically surfacing about a day after authenticating. Autodetect now skips Automatic proxy stubs, and sync refuses to downgrade an existing remote registry entry with a discovered local config. ([a1667e5](https://github.com/velvet-tiger/automatic/commit/a1667e5))

## [1.14.2] - 2026-07-20

### Added

- Each configured MCP server on a project's MCP tab now shows a live/available status pill. Stdio servers are checked by resolving the command on disk or PATH; http/sse servers are checked with a lightweight reachability request. Checked once when the tab loads. ([96446ea](https://github.com/velvet-tiger/automatic/commit/96446ea))

### Fixed

- Closing the main window on macOS no longer quits the app. It now hides the window instead, matching how Mail, Slack and other Mac apps behave; clicking the Dock icon brings the window back. Windows and Linux are unaffected. ([82aad33](https://github.com/velvet-tiger/automatic/commit/82aad33))
- A project's `.mcp.json` could be left with a stale `automatic` binary path after an app move or update, because the repair for this only ran when the desktop GUI launched — not when Claude Code started Automatic's MCP server directly. Restarting MCP servers from Claude Code would then fail for the Automatic server and anything proxied through it. The repair now also runs when `mcp-serve` starts. `.mcp.json` writes are also now atomic, so a crash mid-sync can no longer leave the file corrupted. ([8fd6261](https://github.com/velvet-tiger/automatic/commit/8fd6261))

## [1.14.1] - 2026-07-06

### Fixed

- Switching between projects no longer wipes the top-level Build tab. `selectProject` was clearing `toolEntries` right after loading it, which raced the eager reload triggered by the `selectedName` effect and intermittently dropped the Build tab until the Tools sub-tab was reopened to reload it. ([d7b6807](https://github.com/velvet-tiger/automatic/commit/d7b6807))

## [1.14.0] - 2026-07-03

### Added

- An opt-in per-project toggle keeps a marked block in the project's `.gitignore` listing everything Automatic writes, so generated agent config is not committed. Turning it off removes the block on the next sync. Whole agent directories (`.claude/`, `.codex/`, `.gemini/`, etc.) are ignored; shared tool directories (`.github/`, `.vscode/`, `.zed/`) stay surgical so CI workflows and editor settings are never dropped. A bundled `automatic-gitignore` rule documents the convention while the toggle is on. ([83d5d0a](https://github.com/velvet-tiger/automatic/commit/83d5d0a))

### Removed

- The bundled Spec Kitty plugin has been dropped entirely, including its Rust backend, React panel, and every registry touch point. Documentation that used Spec Kitty as a worked example now uses a generic placeholder plugin. ([6c85c60](https://github.com/velvet-tiger/automatic/commit/6c85c60))

### Fixed

- Clicking the copy-prompt button on a kanban card no longer reorders the card. The click was triggering a drag on pointer-down, and the resulting pointer-up call reordered the card to the end of its column. ([dcf8ee9](https://github.com/velvet-tiger/automatic/commit/dcf8ee9))

## [1.13.1] - 2026-06-28

### Fixed

- The bundled default rules no longer carry framework- or domain-specific content. The `automatic-agent-guidance` rule, which shipped Laravel-specific guidance (Eloquent, FormRequests, Pint, `lando artisan`, translations, `env()`/`config()`, PHPDoc), is now framework-agnostic. Medical-domain naming examples in `automatic-code-style` and `automatic-guardrails`, and niche compliance terms in `automatic-prose`, have been replaced with general ones. Framework specifics belong in language and framework templates. ([b6e43f1](https://github.com/velvet-tiger/automatic/commit/b6e43f1))

## [1.13.0] - 2026-06-28

### Added

- Two new bundled default rules: `automatic-prose` (writing guidance for anything a human reads) and `automatic-agent-guidance` (how the agent should respond to questions and tasks). Both ship with fresh installs. ([33ac764](https://github.com/velvet-tiger/automatic/commit/33ac764))

### Changed

- The guidance for consulting repo-local commands (`.agents/commands-index.md`) now lives in the `automatic-process` rule. The standalone `automatic-commands` rule has been retired. Existing installs drop the orphaned rule automatically on update, and any project references to it are cleaned up. ([2345d5f](https://github.com/velvet-tiger/automatic/commit/2345d5f))
- Every bundled default rule's display name is now namespaced with `Automatic:` so they group together in the UI (for example `Automatic: General`, `Automatic: Service`). ([85c6f0b](https://github.com/velvet-tiger/automatic/commit/85c6f0b))
- The Software Defaults project template now applies the full set of automatic default rules. ([77a1b53](https://github.com/velvet-tiger/automatic/commit/77a1b53))

### Maintenance

- Documentation sync: README feature coverage, `AGENTS.md`, and stale template paths in `CLAUDE.md`. ([41e4bf1](https://github.com/velvet-tiger/automatic/commit/41e4bf1), [dad256f](https://github.com/velvet-tiger/automatic/commit/dad256f), [2d85325](https://github.com/velvet-tiger/automatic/commit/2d85325))

## [1.12.0] - 2026-06-25

### Added

- Support for the Pi (pi.dev) agent. Automatic writes a Pi-scoped `.pi/mcp.json` (Claude-compatible JSON, kept separate from Claude Code's `.mcp.json`) and exposes `.pi/agents/` for Markdown + YAML sub-agent definitions. MCP servers and sub-agents are provided through the `pi-mcp-adapter` and `pi-subagents` community extensions. The project marker is `.pi/`, and `AGENTS.md` is shared rather than owned. ([5472204](https://github.com/velvet-tiger/automatic/commit/5472204))

### Fixed

- Goose projects that use recipes (`.goose/recipes/`) are now detected, not only those with a `.goosehints` file. A bare `.goose/` directory is intentionally not sufficient — the `recipes` subdirectory is required to avoid false positives. The stale MCP documentation that claimed Goose exposes no project-scoped extension config has also been corrected. ([a9bd4d4](https://github.com/velvet-tiger/automatic/commit/a9bd4d4))

### Maintenance

- Added the design-of-record for the 2.0 "Project → Repository" rename, covering the naming map, the idempotent on-disk migrator, the breaking MCP tool rename, and the planned five-PR rollout. ([e001da7](https://github.com/velvet-tiger/automatic/commit/e001da7))

## [1.11.5] - 2026-06-19

### Changed

- The default `automatic-general` rule now states that asking a clarifying question is not the same as receiving permission. Agents must wait for the user to choose explicitly rather than inferring an option from the question itself. ([bb27b25](https://github.com/velvet-tiger/automatic/commit/bb27b25))

### Fixed

- MCP servers installed from the Discover page now honour the explicit `args` array recorded in their install config, restoring missing CLI flags for servers such as Aikido. ([50db601](https://github.com/velvet-tiger/automatic/commit/50db601))
- The library MCP server editor surfaces its own inline "Generate token" link, so users no longer have to bounce back to the Discover page to find it. ([c48d263](https://github.com/velvet-tiger/automatic/commit/c48d263))
- The MCP transport selector in the library editor is rendered as a segmented tab control, replacing the dropdown that previously mixed transports with unrelated options. ([8cc61ed](https://github.com/velvet-tiger/automatic/commit/8cc61ed))
- The "MCP Enabled" control in the library editor is now a proper switch, not an icon-button styled as one. ([893e7e0](https://github.com/velvet-tiger/automatic/commit/893e7e0))
- Rejected OAuth tokens are now shown as a warning state with an explicit "Re-authenticate" action, instead of disappearing silently and leaving the server unconfigured. ([9dae0de](https://github.com/velvet-tiger/automatic/commit/9dae0de))
- The OAuth panel in the library MCP editor is hidden when there is no URL configured and no token exists, removing the empty-state flash that confused new server entries. ([ad08647](https://github.com/velvet-tiger/automatic/commit/ad08647))
- The Projects sidebar now refreshes as soon as a new project is added, instead of waiting for the next foreground drift poll. ([0d78b1d](https://github.com/velvet-tiger/automatic/commit/0d78b1d))

### Maintenance

- Removed documentation that has been migrated to the `automatic-meta` repo, including agent guides, sub-agent format references, remote-source examples, and the cloud-sync and multi-agent plans. `AGENTS.md`, `CLAUDE.md`, and `docs/index.md` now point at `automatic-meta` instead of carrying the content locally. ([1cb929a](https://github.com/velvet-tiger/automatic/commit/1cb929a))
- Aligned the internal cloud-sync contract notes with the shipped webapp implementation. ([12396da](https://github.com/velvet-tiger/automatic/commit/12396da))

## [1.11.4] - 2026-06-09

### Fixed

- Project Group membership is no longer written into each member's instruction file. Embedding peer projects caused churn on tracked files whenever group membership changed and risked leaking sibling project names into public repositories. Related-project context is now exposed only via the `automatic_get_related_projects` MCP tool, and the default `automatic-service` rule directs agents to call it. Existing instruction files have the legacy `<!-- automatic:groups -->` block stripped on their next sync. ([58ea4a1](https://github.com/velvet-tiger/automatic/commit/58ea4a1))

## [1.11.3] - 2026-06-08

### Fixed

- Skills marked as external-only are now deleted from disk when removed, rather than left as orphans. ([afe9f70](https://github.com/velvet-tiger/automatic/commit/afe9f70))
- List and kanban project views now scroll correctly when their contents overflow the viewport. ([e2152e9](https://github.com/velvet-tiger/automatic/commit/e2152e9))

### Maintenance

- Regenerated the full application icon set (macOS, iOS, Android, Windows Store, favicon) from a new 1024×1024 source. ([e50e2fa](https://github.com/velvet-tiger/automatic/commit/e50e2fa))

## [1.11.2] - 2026-06-03

### Fixed

- Windows CI build now succeeds. The `automatic-cli` staging step touches a placeholder file at the `externalBin` path before invoking `cargo build -p automatic-cli`, so the parent crate's `tauri-build` script (which validates every `externalBin` path) does not fail before the real binary has a chance to be produced. The placeholder is overwritten with the real binary once `cli-bin` finishes building, and the subsequent `tauri build` ships the real binary inside the NSIS installer. ([1c0d08c](https://github.com/velvet-tiger/automatic/commit/1c0d08c))

## [1.11.1] - 2026-06-03

### Fixed

- macOS bundle step no longer fails looking for `automatic-cli` at the universal-apple-darwin path. The CLI binary now lives in its own workspace member (`src-tauri/cli-bin/`) so Tauri's bundler — which auto-discovers `[[bin]]` entries in the active crate — only sees the GUI binary. macOS and Linux builds revert to the single-binary path they always used; Windows builds get the CLI via a `tauri.windows.conf.json` `externalBin` entry. ([forthcoming](https://github.com/velvet-tiger/automatic/commit/HEAD))
- Windows compile error in the PATH-broadcast routine. `HWND_BROADCAST` lives in `Win32::UI::WindowsAndMessaging`, and the windows-sys feature flag now correctly enables `Win32_UI_WindowsAndMessaging` and `Win32_Foundation` instead of `Win32_UI_Shell`. ([forthcoming](https://github.com/velvet-tiger/automatic/commit/HEAD))

## [1.11.0] - 2026-06-03

### Added

- New `automatic` command-line interface. Ships as a fourth dispatch mode of the existing binary alongside the GUI, the MCP stdio server, and the MCP proxy. Verbs cover projects, skills, MCP servers, memory, and rules; every command supports `--json` returning the same shapes as the MCP tools, plus `--quiet` and standard exit codes (0 ok, 1 not-found, 2 usage error, 3 I/O). ([729ef53](https://github.com/velvet-tiger/automatic/commit/729ef53))
- New Settings → Command Line page with one-click install. On macOS and Linux it symlinks `/usr/local/bin/automatic` (or `~/.local/bin/automatic` when the system path is not writable) to the bundled binary; install, uninstall, and re-check actions are all idempotent. ([729ef53](https://github.com/velvet-tiger/automatic/commit/729ef53))
- New `automatic init <template>` command. Writes the same files a project sync would write — agent configs, skills, hooks, instruction files, and any inline template `project_files` — into a target directory without ever touching the projects registry, the activity log, or the global MCP server registry. ([65bb91d](https://github.com/velvet-tiger/automatic/commit/65bb91d))
- Windows support for the CLI. Adds a second console-subsystem binary (`automatic-cli`) and a Windows install path that copies it to `%LOCALAPPDATA%\Programs\automatic\bin\automatic.exe` and prepends that directory to `HKCU\Environment\Path`, broadcasting `WM_SETTINGCHANGE` so running shells refresh PATH. No admin or developer mode required. ([40f42c1](https://github.com/velvet-tiger/automatic/commit/40f42c1))

### Changed

- The sync engine's `sync_project_without_autodetect` now delegates to a shared inner helper. The new public `sync::sync_to_directory` runs the same file-writing pipeline without any `save_project` side-effects, so callers that need template-driven setup without a persistent project entry have a first-class entry point. ([65bb91d](https://github.com/velvet-tiger/automatic/commit/65bb91d))

## [1.10.3] - 2026-06-01

### Added

- Projects list now offers a table view alongside the existing grid layout, with a grid/table switcher in the header. ([edbc2fc](https://github.com/velvet-tiger/automatic/commit/edbc2fc))
- Refreshed application icon set. ([d0ebb64](https://github.com/velvet-tiger/automatic/commit/d0ebb64))

### Fixed

- Hardened security posture: enabled CSP, scoped MCP credentials, and added validation for CLI arguments and deep link paths. ([0316210](https://github.com/velvet-tiger/automatic/commit/0316210))
- Opening a project no longer shows a brief empty-state flash before the editor renders. ([e288269](https://github.com/velvet-tiger/automatic/commit/e288269))
- Skill import dialog now stays open after a successful import so the summary remains readable. ([6f424dd](https://github.com/velvet-tiger/automatic/commit/6f424dd), [d33c9d6](https://github.com/velvet-tiger/automatic/commit/d33c9d6))
- `list_spec_kitty_features` now validates `project_dir` before scanning. ([6a3bdef](https://github.com/velvet-tiger/automatic/commit/6a3bdef))
- `build.rs` and CI environment variables are now in sync with `.env.example`. ([78bad11](https://github.com/velvet-tiger/automatic/commit/78bad11))

### Changed

- Split the monolithic `Projects.tsx` into a `Projects` router plus a dedicated `ProjectEditor`, then extracted each editor section (Summary, Agents, Skills, MCP Servers, Tools, Rules, Commands, Hooks, Custom Agents, Project Files, Context, Settings, Memory, Groups, Activity, Recommendations, Docs Files, Docs Links, Docs Notes) into its own panel component. ([48b7ea9](https://github.com/velvet-tiger/automatic/commit/48b7ea9), [e0442a1](https://github.com/velvet-tiger/automatic/commit/e0442a1), [6af1b9a](https://github.com/velvet-tiger/automatic/commit/6af1b9a), [aec6c2a](https://github.com/velvet-tiger/automatic/commit/aec6c2a), [6040380](https://github.com/velvet-tiger/automatic/commit/6040380), [aa3355d](https://github.com/velvet-tiger/automatic/commit/aa3355d), [992b213](https://github.com/velvet-tiger/automatic/commit/992b213), [f247722](https://github.com/velvet-tiger/automatic/commit/f247722), [2af9c7f](https://github.com/velvet-tiger/automatic/commit/2af9c7f), [f74ce9b](https://github.com/velvet-tiger/automatic/commit/f74ce9b), [80ab99d](https://github.com/velvet-tiger/automatic/commit/80ab99d), [2d9dfcb](https://github.com/velvet-tiger/automatic/commit/2d9dfcb), [daf680c](https://github.com/velvet-tiger/automatic/commit/daf680c), [d479373](https://github.com/velvet-tiger/automatic/commit/d479373), [0dfa12f](https://github.com/velvet-tiger/automatic/commit/0dfa12f), [7ec95fc](https://github.com/velvet-tiger/automatic/commit/7ec95fc), [78814f2](https://github.com/velvet-tiger/automatic/commit/78814f2), [4db00cf](https://github.com/velvet-tiger/automatic/commit/4db00cf), [0c520c5](https://github.com/velvet-tiger/automatic/commit/0c520c5), [3a68c9c](https://github.com/velvet-tiger/automatic/commit/3a68c9c))
- Grouped editor-only files under `projects/editor/` and pulled the module-scope helpers into a `projects/` subfolder. ([7d11720](https://github.com/velvet-tiger/automatic/commit/7d11720), [817dc38](https://github.com/velvet-tiger/automatic/commit/817dc38))
- Extracted a shared skill dedup helper and scoped autodetect pruning to project skills only. ([bb3a886](https://github.com/velvet-tiger/automatic/commit/bb3a886))

### Testing

- Added characterization tests for the projects list and editor behavior to guard the refactor. ([1da12dc](https://github.com/velvet-tiger/automatic/commit/1da12dc))

### Documentation

- Added a granular, test-harness-first plan for the Phase 2 projects refactor. ([af36afa](https://github.com/velvet-tiger/automatic/commit/af36afa))

## [1.10.2] - 2026-05-27

### Fixed

- Skill import dialog now reports per-skill whether the import added a new skill or overwrote an existing one, with a count breakdown (for example `1 added · 1 updated`) in the success summary. Re-importing a repository to pick up upstream changes is no longer indistinguishable from a no-op. ([bc1f59c](https://github.com/velvet-tiger/automatic/commit/bc1f59c))

## [1.10.1] - 2026-05-23

### Fixed

- Projects no longer report perpetual skill drift when the same skill exists in both the project's library-backed list and a stale `custom_skills` snapshot. Sync writes library content to disk; drift now uses the library version as its expected-state baseline, and autodetect prunes the obsolete `custom_skills` entries from the project JSON. ([7c76443](https://github.com/velvet-tiger/automatic/commit/7c76443))

## [1.10.0] - 2026-05-22

### Added

- Hooks are now a library item synced per-agent, with templates that can include hooks and a Path handler for referencing an existing script file. ([e1a6c28](https://github.com/velvet-tiger/automatic/commit/e1a6c28), [36bc69f](https://github.com/velvet-tiger/automatic/commit/36bc69f), [c7b9e4b](https://github.com/velvet-tiger/automatic/commit/c7b9e4b), [f3300ad](https://github.com/velvet-tiger/automatic/commit/f3300ad))
- Skills now check for upstream updates on remote-sourced skills, with an "Update now" button in the skill preview. ([a6df6d8](https://github.com/velvet-tiger/automatic/commit/a6df6d8), [f922af0](https://github.com/velvet-tiger/automatic/commit/f922af0))
- New `--icon-command` theme token and aligned Commands summary card. ([cc4fe66](https://github.com/velvet-tiger/automatic/commit/cc4fe66))

### Changed

- Project pickers for skills, MCP servers, and rules now share the "Add from Library" pattern used by commands and hooks. ([f83437f](https://github.com/velvet-tiger/automatic/commit/f83437f))
- Insights moved from the Start section to Tools in the primary navigation. ([244a41d](https://github.com/velvet-tiger/automatic/commit/244a41d))
- Project header actions collapsed to icon-only with hover tooltips, and the project controls bar moved to the bottom of the panel. ([124115a](https://github.com/velvet-tiger/automatic/commit/124115a), [8a57424](https://github.com/velvet-tiger/automatic/commit/8a57424))
- `SummaryMetricCard` collapsed to a single inline row. ([e66ec1f](https://github.com/velvet-tiger/automatic/commit/e66ec1f))
- Dropped shadows and borders from project header icon buttons, header action buttons, and sync buttons for a flatter look. ([96de3d0](https://github.com/velvet-tiger/automatic/commit/96de3d0), [04c9e63](https://github.com/velvet-tiger/automatic/commit/04c9e63))

### Fixed

- Library asset changes (skills, MCP servers, rules, hooks) now propagate to every referencing project on save and upgrade, and drift surfaces failures. ([f2c25dd](https://github.com/velvet-tiger/automatic/commit/f2c25dd))
- Group project references are now cleaned up on delete, rename, and startup. ([0d877b4](https://github.com/velvet-tiger/automatic/commit/0d877b4))
- First-run wizard step labels remain readable in the corporate-dark theme after a step is completed. ([360cb7e](https://github.com/velvet-tiger/automatic/commit/360cb7e))

### Maintenance

- Regenerated the group block in agent instruction files. ([b25a376](https://github.com/velvet-tiger/automatic/commit/b25a376))

## [1.9.0] - 2026-05-19

### Added

- Library Generator: AI-authored skills, commands, rules, and sub-agents can now be generated from inside the app. Adds a `LibraryGeneratorPanel` UI, a new `tools/LibraryGenerator` page, and backend `ai_generate` command/core wiring. (`ec815d7`)
- Top-level Tools section in the primary navigation, with Token Estimator and AI Playground entries surfaced from a new `ToolsHome` page. (`1767dd6`)

### Changed

- Featured content is now nested under Discover instead of sitting as a top-level Community tab. (`c7c7044`)

## [1.8.2] - 2026-05-18

### Fixed

- Native folder/file picker dialogs now work on Windows and Linux. The `open_directory_dialog` and `open_file_dialog` commands previously returned `"not implemented on this platform"` outside macOS, so the wizard "Browse" button and similar buttons in Projects did nothing visible. All three platforms now share a single `tauri-plugin-dialog`-backed implementation. (`ada298c`)
- Wizard project-name extraction now handles Windows backslash paths. The "Where is this project?" Browse flow previously split selected paths on `/` only, so on Windows the auto-filled project name was the full path. (`ada298c`)
- Frontend `invoke("open_directory_dialog" | "open_file_dialog")` call sites now log failures via `console.error` instead of silently swallowing rejections, so future picker errors are visible in the dev console. (`ada298c`)

## [1.8.0] - 2026-05-14

### Added

- Import MCP servers from a pasted JSON snippet. (`07a83a8`)

### Fixed

- Prevent and repair duplicate rule entries in the auto-generated instruction index. The `automatic-checklist` → `automatic-process` migration could leave `["automatic-process", "automatic-process"]` in `file_rules`, causing the rendered AGENTS.md / CLAUDE.md index to list the same rule twice. The migration now drops the old name when the new one is already present, repairs already-damaged `project.json` files in place, and the renderer deduplicates defensively. (`dd20912`)
- Resolve Dependabot security alerts across npm and Rust dependencies. (`3d84f4d`)
- Resolve three CodeQL alerts in YAML escaping and `className` handling. (`14e400c`)
- Validate `GatewayConfig` IDs before URL construction. (`a792976`)
- Pin `@tauri-apps/plugin-dialog` to `^2.7` to resolve version mismatch with Rust crate `tauri-plugin-dialog v2.7.1`, which broke production builds. (`4068153`)

## [1.7.0] - 2026-05-09

### Added

- Add Better Stack MCP server to the Discover catalogue. (`c5a13c7`)

### Changed

- Move all assets under `~/.automatic/library/` and align names with UI labels. (`af7f1bc`)

### Fixed

- Always confirm before switching to unified instruction mode. (`41ed1ca`)

## [1.6.3] - 2026-05-07

### Added

- Add Spacelift MCP server to the Discover catalogue. (`db99200`)

### Docs

- Correct README Supported Agent Tools matrix. (`522d5db`)
- Add SECURITY.md. (`865c9d2`)

### Chore

- Remove stale duplicate MCP server registry from project root. (`d1c9af3`)

## [1.6.2] - 2026-05-06

### Added

- Add Claude 4.7 voice and collaboration skill — default skill capturing voice, intellectual honesty, and collaboration patterns. (`803e449`)

## [1.6.1] - 2026-05-06

### Added

- Add Claude and Codex desktop apps to the project "Open in" menu. (`45e8db3`)

## [1.6.0] - 2026-05-03

### Added

- Add multi-agent abstraction layer and active agent selector. (`da5fb26`)
- Add OpenAI provider and agent label in task log. (`dd81a08`)
- Add GitHub Models provider. (`439b71d`)
- Add Z.ai provider. (`5c310fc`)
- Add OpenCode Zen provider. (`9797846`)
- Add Cloudflare Workers AI provider. (`11d02ab`)
- Per-agent model selection in Settings and updated OpenAI model list. (`a6dafde`)
- Add Cloudflare AI Gateway routing for Anthropic and Workers AI. (`8eb5b34`)
- Extend AI Gateway support to OpenAI. (`a18c17a`)

### Fixed

- Show all agents in active agent selector; use codex icon for OpenAI. (`61b0de0`)
- Style model selector dropdown to match app UI. (`c000d25`)

## [1.5.2] - 2026-05-02

### Added

- Master toggle to enable or disable all in-app AI agent features at once. (`cdf3c9a`)
- Settings panel splits in-app Agents configuration from project Providers. (`8b17e0d`)
- Recently added assets now appear at the top of workspace library lists. (`ed416fa`)
- Library overview page with asset cards for skills, rules, templates, and MCP servers. (`467f483`)
- Discover landing page with asset cards replacing the old Marketplace section. (`cb10489`)

### Changed

- Marketplace renamed to Discover throughout the UI and codebase. (`96c5c90`)
- Apply Template panel replaced with a modal and confirmation view. (`54965c6`)
- Apply-to-Project dropdown replaced with a searchable modal. (`45bb998`)

### Fixed

- Clicking the active section pill now resets in-tab state instead of doing nothing. (`19af064`)
- Section pill navigation always lands on the section's overview page. (`77131bf`)
- New projects always open on the Summary tab. (`fb67a77`)
- Project list badge and project page drift badge now stay in sync; bundled `automatic` skill is auto-healed if it drifts. (`83ea36a`)
- Rules-only templates no longer switch unified instruction mode or overwrite existing instructions. (`2270d8a`)
- Local skills unified into `custom_skills` — removes the divergent `local_skills` field. (`5dfe9f7`)

## [1.5.1] - 2026-04-26

### Fixed

- Toggling Unified instruction mode no longer leaves the per-agent files (`AGENTS.md`, `CLAUDE.md`, ...) in a divergent state. When their content disagrees the user is now shown a picker to choose which file becomes the canonical unified source, with an explicit warning that the unchosen files will be overwritten. Previously, divergence was deferred to drift resolution where adopting one file's content silently cascaded across every other unified target. (`20abecf`)

## [1.5.0] - 2026-04-23

### Added

- Silent sync mode for projects. (`eda40ab`)
- Guardrails default rule. (`31174b3`)
- Styled MCP authorization callback page for OAuth flows. (`cef2be0`)

### Changed

- Skill library relocated to `~/.automatic/library/skills`; legacy skills are migrated on launch and external skills surface in the UI. (`bc0db38`, `d693368`)
- Documentation references updated from `~/.agents/skills` to `~/.automatic/library/skills`. (`784dfed`)
- Process skill now includes an explicit Communicate step. (`4e4e736`)

## [1.4.5] - 2026-04-19

### Added

- Library sidebar items are now grouped under a "My Library" section header. (`dc56ed1`)
- Bidirectional v2 library sync added behind a feature flag. (`e7f58a3`)
- Settings panel now hides the sidebar when active. (`6a6c7eb`)
- Webapp OAuth sign-in added, gated by authentication flag. (`1c0d7ec`)
- `VITE_FLAGS` forwarded to Rust compile-time environment. (`5a54087`)

### Fixed

- Missing project folders are now detected and surfaced in the Projects view. (`7b4ff71`)

## [1.4.4] - 2026-04-15

### Fixed

- Remote installs now preserve and display their GitHub provenance across workspace assets instead of being mislabeled as local. Skills, rules, project templates, MCP servers, and user agents now hydrate author/source metadata from the existing remote provenance registry. (`de61a79`)

## [1.4.3] - 2026-04-15

### Fixed

- Remote install dialog reported "no installable resources" for manifests that delegated their skill list to a referenced `skill.json` (e.g. `aurabx/skills`). `fetch_source_manifest` now resolves the `skills.skill_json` reference and inlines the entries before returning to the frontend.

### Security

- Remote install pathway (`core/remote_sources.rs`) now runs the asset security scanner (`enforce_text_asset`) before writing any skill, rule, template, MCP server, command, or agent to disk, matching the scanning that already happened on the in-app install paths. Skills are walked recursively; symlinks inside a skill tree are rejected outright.

## [1.4.2] - 2026-04-15

### Fixed

- Remote install dialog crashed with `undefined is not an object (evaluating 'p.items.length')` when a manifest omitted any of the `mcp_servers`, `rules`, `templates`, `commands`, or `agents` arrays (e.g. skills-only packages like `aurabx/skills`). (`cde8951`)

## [1.4.1] - 2026-04-13

### Added

- Wire up `automatic://` deep-link URL handler so install links actually open the app. (`920154c`)
- New Remote Install Dialog that fetches the package manifest, shows available resources with checkboxes, and installs selected items.
- Deep-link event bridge from Rust (`on_open_url`) to React frontend via Tauri events.

### Fixed

- `automatic://install?repo=...` links were not functional despite the URL scheme being registered — the deep-link plugin was initialized but had no event listener.

## [1.4.0] - 2026-04-12

### Added

- Remote Sources system for installing resources from git repositories. (`462c22c`)
- `automatic.json` manifest format with JSON Schema for editor validation.
- Support for all resource types: skills, MCP servers, rules, templates, commands, agents.
- `skill.json` fallback for repos that only publish skills.
- Per-agent overrides with include/exclude modifiers in the manifest.
- Version pinning: track a branch, pin to a tag, or lock to a commit SHA.
- Marketplace collections declared in the manifest.
- Monorepo support via `dir` parameter for subdirectory manifests.
- `automatic://` deep-link URI scheme for one-click install buttons.
- Provenance tracking and conflict detection across sources.
- 7 Tauri commands: fetch, install, update, remove, list, conflicts, URI handler.
- Bundled skill: `automatic-remote-source-authoring` for creating source packages.
- Install badge SVGs (light and dark variants) in `docs/assets/`.
- Full documentation with example repository in `docs/remote-sources.md`.

## [1.3.0] - 2026-04-12

### Added

- Add Community section with Featured page. (`9c349db`)
- Add sidebar project removal. (`8ddd6d9`)

### Fixed

- Don't pre-select detected agents in setup wizard. (`6f305c3`)
- Clarify agent sources in wizard. (`7f8e974`)
- Preserve existing instructions on template apply. (`b0b57f8`)

### Maintenance

- Update groups. (`b35039e`)

## [1.2.1] - 2026-04-11

### Fixed

- Rename "New Project" to "Add Project" across sidebar and projects page. (`eff05fa`)
- Improve instruction conflict resolution and diff view. (`be9fada`)

### Changed

- Redesign workspace sidebar to match opencode style. (`2e58f0c`)

### Maintenance

- Docs cleanup. (`fc690e3`)

## [1.2.0] - 2026-04-10

### Added

- Redesign navigation into multi-panel tab layout. (`e567c35`)
- Add collapsible sidebar with toggle icon in top bar. (`3414e5b`)
- Basic security scanning for skills, commands, agents, and templates. (`049fc16`)
- Refine security scanning with context-aware checks and reduced false positives. (`fbf7776`)
- Add security scan status badges and notices to workspace pages. (`c985d7e`)

### Fixed

- Record instruction hashes only for files actually written, preserving unresolved drift conflicts. (`a1f3294`)

### Changed

- Polish ProjectsOverview cards to match GettingStarted style. (`b4c6f69`)
- Merge section tabs and actions into single top bar. (`fd05785`)
- Tidy workspace sidebar spacing and remove Other group. (`05c9c03`)
- Center section toggle pill in top bar. (`85bfbf0`)
- Make health bar flush full-width at top of projects panel. (`9b3c479`)

### Maintenance

- AI docs. (`c62ff80`)

## [1.1.0] - 2026-04-09

### Added

- Add an empty state to Commands in the library. (`4ccd8e2`)
- Add line numbers to text areas and normalise across the application. (`b81880c`)
- Allow selecting file paths in the docs tool. (`f38109d`)
- Add command discovery rule. (`2684a28`)
- Add a commands index.md file. (`41367a9`)
- Commands now live in .agents/commands. (`53143e6`)
- Add Codex plugin scaffold (hidden for now). (`080d3d8`)

### Fixed

- Auto-refresh expired OAuth tokens on 401/403 in proxy. (`da1a6ff`)
- Migrate legacy .clinerules files for Cline agent. (`7319437`)

### Documentation

- Update readme. (`7266410`)

## [1.0.1] - 2026-04-08

### Fixed

- Only add detected agents the user explicitly selected during autodetect. (`3a5eeab`)
- Handle directory paths and update Cline agent adapter. (`e115130`)
- Preserve exact MCP server inputs. (`85bcb6d`)

## [1.0.0] - 2026-04-05

### Added

- Convert Build to a per-project plugin with tool declaration. (`4e671a2`)
- Redesign project nav with Configuration far-right and tools declaring provides_tab. (`559b1e9`)
- Promote Skills, MCP, Agents, Commands to top-level project tabs. (`28100ee`)
- Promote Instructions and Rules to top-level project tabs. (`60b86f3`)
- Promote Memory and Activity to top-level tabs, remove Runtime group. (`8bfbf9c`)
- Add right-aligned project controls bar with Configuration, Insights, Activity, Memory. (`0a216a7`)

### Fixed

- Load activity data when selecting Activity via project controls bar. (`ddb63af`)

### Changed

- Improve ProjectCard layout and add resource counts. (`93fd153`)
- Move 'Write rules to separate files' into a right help sidebar on Rules tab. (`9b98468`)
- Move Groups 'How groups work' callout into a small right help sidebar. (`5806ba3`)
- Move Groups sub-tab from Context to Configuration. (`8550b8a`)
- Remove Build card from project summary tab. (`42bbb04`)

## [1.0.0-beta.7] - 2026-04-04

### Fixed

- Bump the Tauri app config version so generated `latest.json` matches the release version instead of stale beta.4 metadata. (c7f595c)

## [1.0.0-beta.6] - 2026-04-04

### Fixed

- Stop forcing zstd pkg-config in repo config so Windows and macOS cross-target release builds do not fail. (f54a9a1)

## [1.0.0-beta.5] - 2026-04-03

### Added

- Warn when project MCPs conflict with Claude user-scope config (VEL-93). (26d6676)
- Render update notes using MarkdownPreview component. (82067e0)
- Render update notes using MarkdownPreview component. (7377c63)

### Fixed

- Remove dashboard what's new section (VEL-89). (726a300)
- Read release notes from changelog in release workflow. (c6e1a9a)
- Surface revoked MCP OAuth tokens in server UI (VEL-92). (4fac403)
- Use theme-aware scrollbar colors for light themes (VEL-100). (eecdafb)
- Normalise bundled assets into src-tauri/assets/ (VEL-97). (d48937d)
- Sort template lists alphabetically in marketplace and project views (VEL-96). (f4e7d58)
- Sort MCP server list alphabetically in backend (VEL-95). (28125bd)
- Group recommendations by type instead of listing individually (VEL-91). (f349c37)
- Sync project immediately after template application (VEL-90). (242acef)
- Move template merge logic to backend with tests (VEL-90). (6267d46)
- Deduplicate pending recommendations by project+kind+title (VEL-86). (68b98f7)

### Build / CI

- Verify latest.json upload and use stable macOS asset name. (d058d46)

### Maintenance

- Add .cargo/config.toml to fix zstd link on Apple Silicon. (be1be46)
- Fix claude rules. (168458f)

## [1.0.0-beta.4] — 2026-04-01

### Added

- **Instructions index mode**: Rules can now be written as individual files under `.automatic/instructions/` instead of being injected inline into the main instruction file. The instruction file (CLAUDE.md, AGENTS.md, etc.) becomes a short index listing those files, keeping it under ~100 lines. Toggle this per-project from the Rules settings panel. (5f8de37)

### Fixed

- **Split instruction index sync**: When the sync engine writes instruction files to `.automatic/instructions/`, it now correctly injects the index section back into the main instruction file so agents can discover them (bc5ff53)
- **Getting Started checklist**: The checklist is now hidden and What's New expands to full width once all items are complete (b7e3e05)
- **MCP read_skill for local skills**: `read_skill` now resolves project-local skills, not just global registry entries (e782296)

---

## [1.0.0-beta.3] — 2026-03-29

### Added

- **Mandatory automatic-service rule**: The `automatic-service` rule is now enforced as a mandatory project rule, ensuring all projects carry the Automatic MCP service instructions

### Fixed

- **Plugin skill cleanup**: Plugin skills are now deleted when a plugin is disabled, preventing stale skills from persisting (a6e9a11)
- **Project health bar deduplication**: Skill and MCP server counts in the project overview health bar are no longer double-counted (4e0535f)
- **Overview folder collapsed state**: Collapsed/expanded state of overview folders is now persisted to `localStorage` across sessions (efe1a47)
- **Common Docs plugin manifest**: Added `common-docs-write` skill to the Common Docs plugin manifest (c24d531)

---

## [1.0.0-beta.2] — 2026-03-28

### Added

#### Skills
- **Project-scoped custom skills**: Create and manage skills that live within a single project, enabling project-specific agent instructions without polluting the global library (VEL-51)
- **Skill collections**: Group related skills together in the library for easier discovery and organisation (VEL-57)

#### Agents
- **Zed editor support**: Added Zed as a supported agent with SVG icon and sync configuration

#### Plugins
- **Common Docs plugin**: Plugin-provided skills and rules that enrich projects automatically when the plugin is enabled

#### Commands
- **Split editor**: Dedicated description field in the command editor for clearer authoring
- **Rename support**: Inline rename with auto-formatting input for workspace commands
- **Template commands**: Project templates can now include commands that sync to projects

#### Settings & Legal
- **Privacy policy page**: Dedicated sub-page with analytics opt-in toggle
- **Terms of Service page**: In-app terms accessible from Settings
- **Support page**: Quick links to GitHub and Discord support channels
- **Newsletter unsubscribe**: Manage newsletter subscription directly from Settings

#### Projects
- **Sync progress bar**: Visual progress indicator with disabled button state during sync
- **Sidebar order sort**: Default sort option that respects manual sidebar ordering
- **Portable project metadata**: Resolved metadata embedded in project.json for cross-machine portability

#### Navigation
- **What's New section**: Per-release content on the Getting Started page
- **View in library**: Installed MCP servers link back to their library entry from the marketplace
- **MCP marketplace wiring**: Navigate directly to MCP server detail from the marketplace

### Fixed

#### Accessibility
- **WCAG 2.2 form fields**: Compliant border contrast and placeholder text for all form inputs
- **Light theme backgrounds**: Correct white bg-base with proper body background application

#### Skills
- **Multi-repo import**: All skills from multi-skill repositories are now imported correctly (VEL-58)
- **Download icon**: Skill import buttons use a consistent download icon (VEL-59)
- **Panel width**: Skills list panel widened from 264px to 320px for better readability (VEL-60)
- **Bundled avatars**: Automatic graph logo used for bundled skill and provider avatars (VEL-62)
- **Local source badge**: Shows "local" instead of "agentskills.io" for locally-sourced skills
- **Cross-agent deletion**: Skills deleted from all agent directories, not just `agents/claude`

#### Projects
- **Alphabetical sorting**: Agents, skills, and MCP servers sorted alphabetically in project view
- **Sidebar drag order**: Sort now preserves both folder grouping and manual drag order
- **Stats cards**: Clickable stats cards with fixed label truncation
- **Command preview**: Expandable preview and library navigation for workspace commands

#### Plugins
- **Lifecycle fixes**: Plugin skills/rules correctly stripped on removal, enriched on addition, and re-read after save

#### MCP & Agents
- **Platform config dir**: Claude Desktop import uses platform-specific config directory
- **Claude Code servers**: MCP servers correctly read from `~/.claude.json`
- **OAuth error**: False error suppressed when credential storage fails after successful authentication

#### UI
- **Getting Started layout**: Checklist and What's New shown as equal-width columns
- **Collection labels**: Visible in Corporate Dark theme (VEL-63)
- **Command rebuild**: Commands discovered during project rebuild

### Changed

- **Settings restructure**: Support, Privacy, and Terms consolidated into Settings; Appearance extracted into its own section; setup wizard moved to Settings > App
- **Sync mode naming**: `skill_sync_mode` renamed to `sync_mode` with rebranded Settings page (VEL-61)
- **Common Docs**: Auto Docs plugin renamed to Common Docs; skills now fetched from repository
- **Project layout**: Group pills moved to right column under agent icons

---

## [1.0.0-beta.1] — 2026-03-24

### Fixed

- **Windows MSI**: Use a WiX-compatible prerelease version format so Windows bundles can be built successfully.

## [1.0.0-beta1] — 2026-03-24

### Added

- **Projects**: Redesigned the project summary dashboard for a richer overview experience. 
- **Commands**: Added a command library with project command sync support. 
- **Projects**: Rebuild project state from disk to improve recovery and consistency. 
- **Projects**: Added per-project MCP server disable controls. 
- **Templates**: Added sub-agent support to project templates. 
- **Getting Started**: Refined the getting started content and replaced the dashboard entry point with a focused onboarding view. 
- **Navigation**: Added a dedicated library section to the app navigation. 
- **Skills**: Detect Codex skill sources and authors. 

### Fixes

- **Agents**: Onboarding now only auto-selects agents that are actually detected. 
- **Settings**: Restored bundled defaults after erase and reinstall flows. 
- **Onboarding**: Removed the redundant first-project step. 
- **Skills**: Removed the bundled `github-workflow-automation` skill entry. 
- **Analytics**: Record opted-out users without activity tracking. 

### Changed

- **Navigation**: Replaced the dashboard landing flow with the new getting started experience. 
- **Projects**: Moved the Groups tab from Configuration to Context.
- **Plugins**: Replaced the hardcoded tool panel with a registry-driven pattern. 
- **Frontend**: Reorganized `src/` into more logical subdirectories. 

### Maintenance

- **Build**: Fixed export handling. 
- **License**: Added BUSL 1.1 commercial licensing terms. 

### Documentation

- **README**: Refreshed project documentation. 

## [0.13.0] — 2026-03-22

### Features

#### Sub-agents

- **Workspace agents**: Agents can now be configured at the workspace level in `~/.automatic/agents/`, making them available across all projects without per-project configuration
- **Project-local agents**: Agents configured in a project's `.automatic/agents/` directory override or extend workspace-level agents, enabling project-specific tool configurations
- **Agent capabilities**: New `AgentCapabilities` declaration allows agents to advertise supported features (skills, MCP servers, memory, etc.), enabling agents like Spec Kitty to expose specialized capabilities to the Automatic MCP interface
- **Custom agent preservation**: The agent sync process now preserves manually-created agent configuration files, preventing accidental deletion when the sync runs

#### Skills

- **Local skill import**: Skills can now be imported from local directories via the Skills UI, enabling offline skill development and private skill libraries
- **Repository skill import**: Skills can be imported directly from Git repositories by URL, automatically cloning and configuring the skill for use in projects

#### Templates

- **Bundled template import**: Community skills that ship with bundled project templates now automatically import those templates when the skill is installed, reducing setup time for common workflows

#### Projects

- **Group navigation**: Projects belonging to groups now show navigation controls in the Groups tab, allowing quick movement between related projects
- **Group removal**: Remove projects from a group directly from the Groups tab without navigating to the project settings

#### UI

- **Providers tabs**: The Providers page now uses a tabbed interface to separate Installed agents from Available agents, improving navigation clarity
- **Icon refresh**: The Instructions nav icon has been replaced with ClipboardList and terminology updated throughout the UI for consistency

### Fixes

- **MCP seeding**: Marketplace files are now correctly seeded on first launch, preventing missing MCP server entries in fresh installations
- **External links**: Clicking external links now uses Tauri's opener API instead of the default browser, eliminating popup warnings on macOS
- **Recommendations**: Marketplace links and project add flows validate correctly before navigation, preventing errors from malformed URLs
- **Spec Kitty backend errors**: Backend status errors are now displayed in the UI when the Spec Kitty backend fails to start or encounters errors, replacing silent failures with actionable error messages
- **Spec Kitty binary paths**: Explicit binary path overrides are now supported for agents, allowing custom binary locations outside PATH
- **Project instructions**: The Groups section is no longer incorrectly included in generated project instruction files
- **Recommendations navigation**: The recommendations sidebar item has been renamed for clarity; deep-linking to open specific projects now works reliably
- **Group membership sync**: All projects referencing a group are now synced when group membership changes, ensuring agent context stays consistent
- **AI schema**: Missing `additionalProperties: false` added to JSON schema definitions for stricter type safety in AI inference

### Internal

- **Git ignore**: Claude-flow runtime files added to `.gitignore` to prevent tracking daemon state
- **Code style**: Backend Rust modules formatted for consistency

---

## [0.12.1] — 2026-03-19

### Fixes

- **Version**: The 0.12.0 release was tagged before version files were bumped, so distributed binaries identified themselves as 0.11.2. This release corrects that.
- **Onboarding**: Prevent event propagation in the analytics toggle to avoid unintended interactions

---

## [0.12.0] — 2026-03-19

### Features

- **Project Groups**: Projects can now be organised into groups; group membership is injected into agent context as a Related Projects section
- **Archive/Unarchive Features**: Features can now be archived and unarchived via the UI and MCP tools
- **Resizable Columns**: Feature list columns are now resizable
- **`automatic_get_related_projects` MCP tool**: Exposes related projects from a group to agents via the MCP interface

### Changed

- **Feature Docs**: Add forms in Files & Dirs and Links tabs are now collapsed by default to reduce visual noise

---

## [0.11.2] — 2026-03-18

### Fixes

- **MCP Servers**: Remote servers with API key auth (e.g. Upsun) now correctly show an environment variable editor instead of the OAuth flow after installation
- **MCP Marketplace**: Installing a remote server that requires an API key now seeds the env var fields in the saved config so the token can be entered immediately

---

## [0.11.1] — 2026-03-18

### Features

- **Marketplace**: MCP servers, templates, and collections can now be loaded from `~/.automatic/marketplace`, enabling local marketplace customisation
- **Marketplace**: Upsun MCP server added to the marketplace directory
- **Projects**: AI update instruction command added to projects for easier agent onboarding

### Fixes

- **Agents**: Description now displays correctly when MCP capability is disabled
- **Kiro**: Skills are written to `.kiro/skills/` per the agent skills specification
- **Warp**: `project_file_name` corrected to `AGENTS.md`
- **Cline**: `project_file_name` corrected to `.clinerules`
- **Antigravity**: `project_file_name` corrected to `GEMINI.md` and MCP write disabled
- **Goose**: Architecture corrected and YAML parser bugs fixed

### Internal

- Agent reference documentation added for all researched agents
- AGENTS.md problem-solving process and making changes guide updated

---

## [0.11.0] — 2026-03-16

### Features

- **Marketplace**: Aikido Security MCP server added to the marketplace with a companion skill that installs automatically
- **OpenCode**: Cache and snapshot cleanup actions added to the Providers > OpenCode settings panel
- **Projects**: Clicking the active nav item now returns to the projects list, making navigation more predictable

### Fixes

- **MCP Servers**: Arguments section is now always shown for local stdio servers, preventing hidden configuration state
- **Sync**: Claude project files are cleaned up correctly when an agent is removed from a project

### Changed

- **Features**: Kanban columns now expand to fill the full width; card title line height tightened for density

---

## [0.10.1] — 2026-03-14

### Features

- **Settings**: Reinstall Defaults button added to restore bundled rules, templates, skills, and MCP server configurations

### Fixes

- **Sync**: Projects are now automatically re-synced on startup when the app binary path changes (e.g. switching from a dev build to the release app, or after an update), eliminating false "modified" drift on `.mcp.json` and `opencode.json`
- **Sync**: `automatic` skill files (`SKILL.md`) are now written to disk on the first launch after a project is created, fixing "missing" drift that appeared immediately after a clean sync
- **Features**: Create feature submission restored after being temporarily broken
- **Projects**: Delete actions for attached items now remain visible
- **Projects**: Features view no longer persists when navigating to Tools
- **Projects**: Project creation wizard stub reference is properly cleared after successful creation

### Documentation

- **Project Templates**: Terraform template instructions updated with improved guidance

---

## [0.10.0] — 2026-03-12

### Features

- **Updater**: Background hourly update check with silent download and a restart toast notification when a new version is ready
- **Projects**: Documentation tab added with Files, Links, and Notes sub-tabs for project-level reference material
- **Projects**: Instructions tab renamed to Context for clarity
- **Tools**: Tools architecture introduced with a plugin framework and agent tool auto-detection
- **Plugins**: Spec Kitty plugin added with a features list and work-in-progress Kanban board
- **UI**: Reusable token estimate display added across all instruction and context editors

### Fixes

- **Projects**: Rules summary card layout aligned with Skills and MCP Servers cards
- **Projects**: Documentation tab layout aligned with the project detail panel structure
- **Context**: Documentation index split from generated context to avoid coupling

### Refactored

- **Plugins**: Plugin commands and manifest abstracted behind a clean plugin boundary

---

## [0.9.4] — 2026-03-11

### Features

- **Drift**: Show on-disk `SKILL.md` content in the stale skill modal for accurate review before taking action
- **Drift**: Adopt, remove, or overwrite actions added to stale skill directories
- **Rules**: Built-in rules are now locked with a Built-in badge and read-only indicator; a Duplicate button creates an editable copy
- **Rules**: `automatic-checklist` rule is automatically migrated to `automatic-process` on install
- **Features**: Copy prompt button added to detail panel header
- **Features**: Fields auto-save atomically — no manual save button required
- **Features**: Floating toast confirmation shown on prompt copy
- **Projects**: Project listing sidebar shown on overview page with a folder context filter
- **Projects**: Sync all button added to overview, syncing all drifted projects in one action

### Fixes

- **Rules**: `automatic-` prefix is now stripped when duplicating built-in rules
- **Features**: Selected feature is cleared when switching projects
- **Features**: Feature ordering is stable across Kanban and list views
- **Features**: Partial ID lookup supported in `get_feature` MCP tool
- **Templates**: Applied rules are stored under the `_project` key so they appear in the Rules tab

---

## [0.9.2] — 2026-03-10

### Features

- **Features**: Tags field on feature cards replaced with a pill-based `TagInput` component with autocomplete from existing project tags
- **Features**: Prompt button added to list rows and Kanban cards; copies a compact prompt containing the ticket ID, name, and instruction to fetch the full feature via the Automatic MCP

### Fixes

- **Features**: Prompt button repositioned to the Kanban card header row, beside the priority dot, for consistent placement
- **Features**: Prompt button is icon-only and always visible (removed conditional visibility)
- **Features**: Prompt text simplified to ticket ID, name, and a single MCP fetch instruction
- **Features**: Literal `\n` escape sequences in stored descriptions are now correctly unescaped on load
- **Features**: Description panel defaults to rendered Markdown preview (not raw edit mode) when opening a feature
- **Features**: "In Progress" badge no longer truncated — State column width increased to fit the label
- **Features**: List view title column no longer hard-truncated at a fixed max-width
- **MCP Servers**: Duplicate MCP server registrations that silently dropped tools have been removed
- **MCP Servers**: Beta notice banner now uses semantic warning design tokens instead of hardcoded colours

### CI

- Derive the macOS artifact filename dynamically in the release workflow, fixing updater 404 errors caused by a mismatched static filename

### Chores

- Remove obsolete `daemon-state.json` file

---

## [0.9.1] — 2026-03-10

### Features

- **Projects**: Enforce the built-in Automatic MCP server and skill across all projects on startup — both are registered in the global registry, written to `~/.mcp.json`, and assigned to every project automatically
- **Projects**: Built-in MCP server and skill are marked as protected; they cannot be deleted or removed via the UI (MCP Servers page, Skills page, project selectors) or the backend

### CI

- Remove duplicate macOS asset upload step — `tauri-action` already uploads `Automatic.app.tar.gz`; the redundant manual upload has been removed to prevent duplicate release assets
- Rename `uploadUpdaterJson` to `includeUpdaterJson` in the release workflow to match the current `tauri-action` API

---

## [0.9.0] — 2026-03-10

### Features

#### Features Tracking
- Per-project feature tracking with a full Kanban board UI and build list view
- MCP tools for agents to create, list, update, and claim features with assignee enforcement
- Build list columns are sortable; build filters persist per project
- Build view preference remembered per project
- New features open in the side panel for focused editing

#### Projects
- Project list sidebar auto-hides with a hover flyout and a pin toggle to keep it open
- Project tray opens on click (not hover) and closes automatically when unpinned after selection
- Project tabs grouped into collapsible sections for a cleaner layout

#### Skills
- Version-gated reinstall of bundled skills on app update ensures the latest built-in skill is always deployed
- Remote (store-installed) skills are now locked for editing; a Duplicate button creates an editable copy
- Bundled Automatic skill includes frontmatter and `skill.json` metadata

#### Recommendations
- System check added for missing `.automatic/context.json` to surface setup gaps early
- Two-phase AI inference approach guarantees structured JSON output from the recommendations engine

#### Conflicts
- Instruction file conflict modal now shows a full line-level diff for precise change review

### Fixes

- **Features**: Kanban drag-and-drop replaced HTML5 DnD with pointer-event handling to fix reliability on macOS
- **Recommendations**: Missing `metadata` column added to test schema, preventing DB errors
- **Skills**: `stopPropagation` call removed that was blocking clicks on GitHub links in skill cards
- **Projects**: OpenCode restart warning removed from the MCP Servers tab (no longer applicable)
- **Keychain**: Debug builds now use a separate keychain service name to avoid colliding with production credentials

### Tests

- 113 new unit tests added across 7 previously untested Rust modules

### Chores

- Skill Store: replaced Website Audit with Skill Creator in the featured skills list
- Updater: public key in `tauri.conf.json` refreshed

## [0.8.0] — 2026-03-08

### Features

#### Onboarding
- Anthropic API key step added to the first-run wizard with keychain storage and an obfuscated hint display

#### Dashboard
- Projects health bar added above the use cases section, showing overall project health at a glance

#### Projects
- Two-column summary layout with a rules widget and elevated setup callout
- Folder grouping with compact cards on the overview page

#### Context
- AI-powered project context generation with an integrated task log panel
- Project context exposed via a dedicated MCP tool (`get_project_context`)
- Context storage migrated from TOML to JSON; Tauri commands registered

#### Instructions
- AI generation for project instruction files with reactive recommendations
- Externally-modified instruction files are detected to prevent accidental overwrites
- Conflict modal simplified to a summary view with aligned `DriftReport` types

#### Rules
- Rules moved to a dedicated project tab with project-level scope
- Custom rule editor on the Rules tab with a dropdown global rule picker
- Inline custom project rules support
- Rules correctly routed to `.claude/rules/` in all write paths
- Selected rule indicators replaced solid brand fill with subtle highlights

#### Recommendations
- AI skill and MCP server suggestions with a proper install flow
- Compact single-line rows with a collapsible description toggle
- Recommendations now recompute on project save
- Rules recommendation copy updated to reflect the Rules tab

#### Task Log
- Task log entries persisted to disk
- Header toggle button and copy actions added

#### Agent / AI Playground
- API key management and live model list for AI Playground
- Library read and marketplace search tools added to the built-in agent
- Generate buttons gated on API key presence; environment variable fallback removed

#### Skills
- `skill.json` support added per the velvet-tiger/skills-json spec

#### Plugins
- Plugin framework introduced with a new Settings > Plugins page

#### Sidebar
- Navigation reorganised; Agents section renamed to Providers

### Fixes
- Theme: improved cyberpunk primary button text contrast

### Chore
- Version bumped to 0.7.0 with changelog (included in prior release)

---

## [0.7.0] — 2026-03-06

### Features

#### Projects
- Folder/group support in the project sidebar with overview tile sync
- Apply multiple templates to a project at once from the template selector

#### Markdown
- Blockquote rendering in MarkdownPreview components

### Fixes
- Wizard now deletes the stub project when the user cancels or navigates away
- Template rules are applied correctly even when `unified_instruction` is empty
- Sidebar and "New Project..." item are hidden while a project is being created

### Build
- Dev server port changed from 1420 to 1421

---

## [0.6.0] — 2026-03-05

### Features

#### Recommendations
- New system recommendations engine that evaluates projects for missing rules and instruction files, surfacing actionable items per project
- Dedicated Recommendations page (sidebar nav, Configuration section) grouped by project with dismiss support
- Per-project Recommendations tab in the Projects view
- Dashboard banner linking to the Recommendations page (replaces full inline list)
- SQLite-backed recommendations store with priority levels (low/normal/high) and full lifecycle management (pending/dismissed/actioned)

#### Memory
- Claude Code auto-memory integration: read-only access to `~/.claude/projects/<hash>/memory/` files directly in the Memory tab (requires Claude agent on project)
- Per-file Promote button to save Claude auto-memory entries into Automatic's structured memory store
- `automatic_read_claude_memory` MCP tool so agents can inspect Claude's learnings
- Memory mutations (store/delete/clear) now logged to the Activity feed and analytics

#### Agents
- Per-agent configuration options framework: inline collapsed settings panel per agent card with a chevron toggle
- `claude_rules_in_dot_claude` option (default: on) syncs rules to `.claude/rules/<name>.md` files instead of injecting inline into `CLAUDE.md`
- Default agent options configurable in Settings; new projects seeded from saved defaults

#### Projects
- Health bar summary strip above the card grid showing total projects, synced/drifted counts, unique agent count, skills in use, and MCP server count
- Segmented sync-health progress bar that fills as drift checks complete
- Project Instructions: Rules moved from footer strip to right sidebar

#### Skills
- Bundled `automatic-llms-txt` skill for creating `llms.txt` files following the llmstxt.org standard; auto-installed on first launch

### Fixes
- Drift detection now uses raw SKILL.md content for comparison, eliminating false positives
- Folder picker on macOS replaced with an `osascript`-backed command, fixing a panic on Apple Silicon caused by `NSOpenPanel` returning NULL (rfd #259)

### CI
- AMPLITUDE_API_KEY secret passed to Tauri build steps
- Workaround for tauri-action `latest.json` bug affecting universal macOS builds

---

## [0.5.0] — 2026-03-03

### Features

#### Onboarding Wizard
- Auto-detect installed agents and import global MCP server configs during first-run setup

#### Project Templates
- Software Defaults added as a bundled project template

#### Projects
- Search filter on the projects overview
- Sort toggle with last-activity ordering
- Inline MCP config preview with deep-link to the server detail page
- Redesigned project detail page layout

#### Activity
- Activity logging API and frontend integration with Recent Activity display on the dashboard

#### MCP Servers
- 67 additional servers from the Anthropic registry added to the MCP Marketplace, sorted alphabetically with contextual counts and expanded transport filters
- Environment variable values encrypted at rest; shell environment variable inheritance supported
- Per-row reveal toggle to mask/unmask env var values in the editor
- Beta notice banner shown on library MCP server detail views

#### Settings
- Reset and erase data actions

### Style
- Dashboard Discover & Extend cards aligned with use case card layout
- Animated TechMeshBackground removed from the dashboard
- OpenCode restart notice downgraded from warning to info

### Fixes
- Configuration dashboard updated to 3-column grid; MCP server sidebar border tokens corrected
- Window drag region restored during first-run wizard
- Card layout improved and status pill aligned in project card footer
- Dashboard row heights made consistent between Activity and Projects panels
- Sync button no longer navigates back to the projects list after syncing
- Empty state flash and badge height shift prevented on projects overview
- Dark theme icon colours corrected


---

## [0.4.0] — 2026-03-03

### Features

#### Skills
- Inline skill preview directly on the project Skills tab
- Fork-to-local action to copy a marketplace skill into the local skill library
- View-in-library shortcut to jump from a project skill to its Skills entry
- "Used By" right sidebar on skills showing linked projects and templates as clickable badges
- GitHub owner avatar displayed as skill logo with letter fallback
- License field support on skills
- Bundled Automatic skills with correct authorship attribution
- Core skills auto-installed on first run; template skills installed on demand

#### Project Templates
- "New project from template" action that opens the project wizard with the template pre-applied
- Applied To project badges on template detail pages are now clickable links
- Imported pill shown on template marketplace cards

#### Marketplaces
- Collections marketplace added, featuring the Automatic Skills collection
- Amplitude and Amplitude EU MCP servers added to the MCP Marketplace
- Marketplace-imported MCP server core settings are now locked to prevent accidental edits
- Skills marketplace renamed from "Skills Store" throughout the UI
- Consistent 3→4 column responsive grid across all three marketplaces
- Standardised search box layout across marketplaces

#### MCP Servers
- OAuth 2.1 PKCE proxy for authenticating remote MCP servers

#### Dashboard
- "How Automatic helps" use cases section added
- Featured section reworked with marketplace card template
- Getting Started section reorganised into a conditional right-column checklist
- Getting Started checklist flags persisted to `settings.json`
- Welcome message copy improved

#### Configuration
- Configuration Overview dashboard showing a summary of all configured resources

#### Projects
- Project overview replaced with full-width card grid layout
- Sync status badge pinned to the right edge of card headers

#### Theme
- Follow system light/dark preference by default
- Dark backgrounds lightened one zinc step for improved readability
- Always apply dark theme when disabling follow-system (no accidental light flash)
- Coral theme extended with a warm-tinted neutral palette
- Dark and corporate-dark theme backgrounds lightened; corporate-dark text brightened
- Muted text contrast improved in dark and cyberpunk themes

#### Developer
- Debug builds now use a separate `.automatic-dev` data directory to isolate dev state from production
- Minimal feature flag framework added (`flag()` helper in `flags.ts`)
- AI Playground view added (behind `ai_playground` feature flag)

### Fixes
- Stale marketplace plugin path resolved on fresh install
- Inner border removed from Getting Started items (fixes bottom border clipping)
- GitHub avatar fetch skipped for bundled skills (avoids 404 noise)
- Drift detection and sync now use a unified server map (eliminates false drift reports)
- Template `_author` propagated correctly on import and cleared on manual edit
- OpenCode MCP warning updated to note a restart is required for new servers
- MCP Marketplace "Add to MCP Servers" button uses white text in dark theme
- Folder icon colour on corporate-dark theme uses `icon-agent` token
- Traffic light position aligned to OS standard position
- Ship icon uses `icon-agent` colour token for theme consistency
- Projects sync badge pinned to right edge of card header
- Unused `sync_projects_referencing_rule` function removed

### Dependencies
- Switched HTTP backend to `native-tls` and removed unused dependencies

### CI
- `VITE_BRANDFETCH_CLIENT_ID` passed to `tauri-action` build steps

### Docs
- Windows build workaround documented for Rust on Parallels

### Chore
- `.claude-flow` daemon state and PID files removed from repository
- `github-release-management` skill removed from repository

---

## [0.3.0] — 2026-03-01

### Features

#### Themes
- Added Accessible theme with WCAG AA+ and colour-vision-deficiency-safe palette
- Added official Dark and Light themes as defaults
- Added Corporate Dark and Corporate Light themes (renamed from Sleek)
- Dynamic semantic icon colours with per-theme token mappings
- Dark agent icons rendered correctly in light themes

#### Dashboard
- Restructured layout with welcome note and featured cards using compact AuthorPanel layout

#### Author & Marketplace
- Author metadata added to marketplace templates, MCP servers, and skills
- AuthorPanel component with session caching and rate-limit fallback

#### Rules
- Per-project sync status indicators and Update buttons
- Automatic MCP Service rule renamed to "Automatic" and auto-enabled by default

#### Agents
- AgentCapabilities declaration for supported feature advertising
- Agent lists now sorted alphabetically by label

#### Memory
- Replaced inline memory tab with scalable MemoryBrowser component

#### Projects
- Summary tab redesigned with actionable layout

#### Onboarding Wizard
- "First project" step added to the onboarding flow
- Cancel support for re-opened wizard; mesh hidden on minimal themes

#### Editors
- JetBrains IDEs added to the "Open In" list

#### MCP Server
- Server metadata populated with title, description, and URL

#### UI
- App version displayed in sidebar footer
- Delete confirmation dialogs for MCP servers, templates, rules, and skills

### Refactoring
- Rust backend decomposed from monolithic `core.rs`/`lib.rs`/`sync.rs` into a modular directory structure
- Frontend localStorage keys migrated from `nexus.*` to `automatic.*` namespace
- All remaining Nexus references renamed to Automatic across the Rust backend

### Fixes
- AgentSelector Add button styling corrected to match other selectors
- Rules sync status check resolved for unified mode and section-only comparison
- Wizard step indicator layout restored; invalid border token classes removed
- Icon theme tokens used correctly in empty state icon boxes
- Agent icon filter corrected on light themes
- `allow-start-dragging` capability added to enable window drag
- Color contrast improved across light themes
- Markdown table borders lightened; inline code visibility fixed in Sleek theme
- Welcome link visibility improved on Corporate Dark
- Icon and rule pill contrast improved for Corporate Dark and Light themes

### CI
- Alternative Apple environment variables passed for notarization
- Keychain hang during codesign resolved

---

## [0.2.0] — 2026-02-28

### Features

#### Shell & UI
- Custom overlay titlebar with Linear-style header layout replacing the default macOS titlebar
- Sidebar logo text increased in size and rendered in pure white
- Add buttons restyled as bordered pill shapes for improved visibility

#### Onboarding
- First-run setup wizard with Attio newsletter subscription step

#### Projects
- Yellow folder icon in the sidebar indicates projects with configuration drift
- Default Agents setting pre-populated into new projects

#### Settings
- Sub-page sidebar navigation for structured settings sections
- Default Agents global preference

#### Analytics
- Amplitude analytics integration with user opt-out support
- Analytics events routed through Rust backend to the Amplitude EU endpoint

#### CI / Distribution
- macOS code signing and notarization in the release workflow

### Fixes
- Analytics events now route correctly through the Rust backend to the EU endpoint
- Attio debug logging removed from newsletter integration
- Settings sub-page sidebar text brightened for legibility
- Error boundary and global error handlers added to surface black-screen failures
- ClerkProvider updated to allow Tauri origins; `.env.example` added

---

## [0.1.0] — 2026-02-27

Initial public release of Automatic — a desktop hub for AI coding agents.

### Core concepts

- **Hub, not executor** — Automatic does not run agents. It exposes an MCP server (stdio transport) that external tools (Claude Code, Cursor, custom agents) connect to in order to pull skills and sync configuration.
- **Skills** — reusable instruction sets with optional companion resources that agents load on demand via the MCP interface.
- **Projects** — workspace configurations that map a local directory to a set of agents, MCP servers, and skills.
- **Memory** — per-project key/value store that agents use to persist context across sessions.
- **Rules** — reusable content blocks that are injected into project instruction files.

### Features

#### Projects
- Three-step project creation wizard
- Auto-detection of installed agents (Claude Code, OpenCode, Codex, Cursor, Kiro, Goose, Warp, Antigravity, and more)
- Agent-specific SVG logos throughout the UI
- Editable project description and directory from the Summary tab
- Memory management tab per project
- One-click MCP server sync to agent config directories
- Skill sync with copy and symlink modes
- Inline editing of local skills within a project
- Unified project instructions and rules generation per template

#### Skills
- Full CRUD skill editor with frontmatter fields (name, description, tags)
- Companion resource discovery and display
- Skill Store integration — browse and install community skills from skills.sh
- Bundled marketplace template skills

#### MCP Marketplace
- Directory of 40 MCP servers with search and category filters
- One-click install into project configuration
- Brand icons via Brandfetch CDN
- Template dependency checking

#### Template Marketplace
- Browse and apply project templates
- Brand icons and indigo-unified theme

#### Dashboard
- Animated tech mesh background
- Getting Started section shown when no projects exist
- Marketplace shortcut cards
- Memory stat card in the project summary grid

#### Settings
- Skill sync mode configuration (copy vs symlink)
- Auto-update via `tauri-plugin-updater` — checks GitHub Releases for new versions, shows release notes, and prompts restart after install

#### MCP Server (agent interface)
- Five tools exposed over stdio transport: `list_skills`, `read_skill`, `list_projects`, `read_project`, `list_mcp_servers`
- Memory tools: `store_memory`, `get_memory`, `list_memories`, `search_memories`, `delete_memory`, `clear_memories`
- Credential retrieval: `get_credential`
- Session tracking: `list_sessions`
- `sync_project` tool — writes agent-specific MCP config files to the project directory

### Fixes
- Correct re-detection of Kiro, Goose, and Antigravity after agent removal
- Prevent removed agents from being re-added on project save/load
- Skill symlink now targets the skill directory, not individual files
- Skill fetch handles mismatched directory names
- Native Tauri dialog used for project deletion confirmation
- Warp removal correctly deletes `WARP.md` via owned config paths
- Junie removal deletes the entire `.junie/` directory

[0.10.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.10.0
[1.0.0-beta.2]: https://github.com/velvet-tiger/automatic/releases/tag/1.0.0-beta.2
[1.0.0-beta.1]: https://github.com/velvet-tiger/automatic/releases/tag/1.0.0-beta.1
[1.0.0-beta1]: https://github.com/velvet-tiger/automatic/releases/tag/1.0.0-beta1
[0.9.1]: https://github.com/velvet-tiger/automatic/releases/tag/0.9.1
[0.9.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.9.0
[0.8.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.8.0
[0.7.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.7.0
[0.6.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.6.0
[0.5.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.5.0
[0.4.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.4.0
[0.3.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.3.0
[0.2.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.2.0
[0.1.0]: https://github.com/velvet-tiger/automatic/releases/tag/0.1.0
