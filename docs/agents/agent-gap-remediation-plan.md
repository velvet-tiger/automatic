# Agent Gap Remediation Plan

Work list closing the gaps in [upstream-audit-2026-07-30.md](./upstream-audit-2026-07-30.md).

Scope decisions taken: `automatic-app` only, full audit scope, the Zed
instruction-file migration included, and all three incidental data-loss defects
included. `automatic-cli` holds an independent byte-identical copy of the agent
module and needs the same changes as a separate piece of work.

Each phase below is independently reviewable. Dependencies are stated so phases
can be handed to different people. Anything marked **blocks** must land first.

## Context

Three agents are broken now. Kilo Code rebranded to Kilo and replaced its config
format, so Automatic writes a file it no longer reads. Zed has no `.zed/agents`
directory, so Automatic writes sub-agent files nothing loads. Junie's
`sync_skills` disagrees with its `skill_dirs`, leaving `.junie/skills` outside
drift detection.

Four vendors shipped features Automatic does not expose. Gemini CLI, GitHub
Copilot and Droid added lifecycle hooks. Copilot, Droid and Kiro added custom
sub-agents. Codex's hook event list grew from 6 to 11, and five of those events
are dropped at sync time today.

### The root cause, stated once

The sync engine does not gate on `capabilities().agents` or
`capabilities().commands`. Sub-agent sync runs if `agents_dir()` returns `Some`
([engine.rs:565](../../src-tauri/src/sync/engine.rs)). Command sync runs if
`commands_dir()` returns `Some` ([engine.rs:622](../../src-tauri/src/sync/engine.rs)).
Only `capabilities().hooks` gates anything
([engine.rs:483](../../src-tauri/src/sync/engine.rs) and `:512`). `capabilities()`
is otherwise only serialised to the UI in
[commands/agents.rs:50](../../src-tauri/src/commands/agents.rs).

A flag without a method lies to the user. A method without a flag is invisible.
Zed is the first kind. Copilot, Droid and Kiro are about to be the second. Phase 1
adds a contract test that turns both into a test failure.

---

## Phase 1 — Foundations and merge safety

**Blocks:** every other phase. **Effort:** high judgement. Blast radius is every
agent, and the acceptance criterion is that existing tests still pass.

### 1a. Derive `sync_skills` from `skill_dirs()`

- [x] Add a default `sync_skills` to the trait in `agent/mod.rs` that loops
      **every** entry of `skill_dirs(dir)` and calls the existing
      `sync_individual_skills` ([agent/mod.rs:520](../../src-tauri/src/agent/mod.rs))
- [x] Delete all 16 per-agent `sync_skills` overrides
- [x] Test: drift is quiet for a Junie project in `symlink` mode
- [x] Test: drift is quiet for a Junie project in `copy` mode

Why loop every entry rather than `skill_dirs()[0]`: `sync_skills` is called only
by `collect_skills_drift` ([drift.rs:619](../../src-tauri/src/sync/drift.rs)),
which writes into a tempdir and then iterates `skill_dirs(dir)` comparing
`tmp/<relative>` against disk. Any path `sync_skills` misses is a directory drift
never checks. Using `[0]` would move Junie's blind spot to `.agents/skills`
instead of removing it.

Junie's `.junie/skills` becomes drift-checked for the first time here. Verify both
sync modes before shipping. Getting this wrong lights up drift for every Junie
project on first run after release.

### 1b. Seed the MCP drift check

- [x] Add an `mcp_merge_inputs()` trait method returning the paths an agent merges
      into
- [x] Have `collect_mcp_drift` ([drift.rs:530](../../src-tauri/src/sync/drift.rs))
      copy those files from the project into the tempdir before calling
      `write_mcp_config`

`collect_mcp_drift` does a non-recursive `read_dir` of the tempdir at
[drift.rs:551](../../src-tauri/src/sync/drift.rs), so only root-level config files
have ever been MCP-drift-checked. Without this seeding, 1c produces permanent
false drift for every OpenCode project.

Do not make `collect_mcp_drift` recursive. That would newly drift-check eight
agents' nested config files at once, on top of these migrations. Separate work.

### 1c. Stop the clobbers

- [x] `opencode.rs::write_mcp_config` — read and merge instead of building with
      `json!({...})`. Preserve the user's `model`, `permission`, `instructions`
      and `agent` keys
- [x] `gemini_cli.rs:81` — return `Err` on a JSON parse failure instead of
      falling back to `Map::new()`, matching
      [claude_code.rs:288](../../src-tauri/src/agent/claude_code.rs)
- [x] `zed.rs:58` — same fix
- [x] `github_copilot.rs:89` — same fix. **Not in the original audit.** The
      table-driven `a_malformed_target_config_is_an_error_not_a_clobber` test
      from 1f found it: `.vscode/mcp.json` had the identical
      `_ => Map::new()` fallback. Four clobbers, not three
- [x] `warp.rs:93` — drop `AGENTS.md` from `owned_config_paths`

`AGENTS.md` is shared with seven other agents. It currently lands in the managed
`.gitignore` block via [agent/mod.rs:428](../../src-tauri/src/agent/mod.rs), and
the default `cleanup_mcp_config` would delete it.

The three JSON fixes share one helper, `read_mergeable_json_object`, so a fifth
merge writer cannot reintroduce the bug by copy-paste. Absent and empty files
still yield an empty object; only unparseable content is an error.

### 1d. Zed sub-agents

- [x] Delete `agents_dir` (`zed.rs:194`)
- [x] Add `agents: false` to `capabilities()`

Both are needed. The method drives sync, the flag drives the UI badge.

### 1e. Extract the OpenCode-dialect MCP writer

- [x] Move `opencode.rs::write_mcp_config` body into `agent/mod.rs` as
      `write_opencode_dialect_mcp_config(path, schema_url, servers)`
- [x] Move `normalise_import` into `agent/mod.rs` as
      `normalise_opencode_dialect_server(config)`, keeping it a plain `fn` pointer
      so `discover_mcp_servers_from_json` still accepts it

Phase 2 consumes both. The extracted writer takes a full path and creates its
parent directory, so Phase 2 can point it at `.kilo/kilo.json` unchanged.

### 1f. Contract tests

- [x] New `agent/contract_tests.rs`, table-driven over `all()`
- [x] `agents_dir_and_the_agents_capability_agree`
- [x] `commands_dir_and_the_commands_capability_agree`
- [x] `every_skill_dir_is_inside_the_project_dir`
- [x] `sync_skills_populates_every_skill_dir`
- [x] Capability-matrix snapshot test: serialise `(id, label, project_file_name,
      capabilities, agents_dir?, commands_dir?, skill_dirs)` for `all()` into a
      stable table compared against an inline expected string
- [x] `mcp_format_tests.rs`: `merge_preserves_unrelated_top_level_keys` (seed
      `{"_userKey":"keep"}`, assert survival)
- [x] `mcp_format_tests.rs`: `a_malformed_target_config_is_an_error_not_a_clobber`

Both `mcp_format_tests` additions iterate `mcp_merge_inputs()` rather than a
hand-listed set of agents, so a merge writer added later is covered on the day
it declares its inputs. Each asserts a non-zero count so neither can pass
vacuously.

The snapshot test is the highest-value item here. Every later phase shows up as
one reviewable diff in one place. That is exactly what was missing when Zed's
`agents: true` went stale.

### Phase 1 status

Landed. 779 tests pass, `cargo clippy --all-targets -- -D warnings` is clean,
`make check` is clean.

Still outstanding: the two manual end-to-end checks in the Verification section
below. Both need a registered project and a real sync, which mutates the user's
Automatic state and project directories, so neither was run. The automated
equivalents cover the same invariants at the collector level:

| Manual check | Automated stand-in |
|---|---|
| 1a — Junie drift clean in both sync modes | `junie_skill_dirs_are_quiet_after_a_symlink_mode_sync` and `..._copy_mode_sync`. These build both on-disk shapes directly, because `symlink_skills_from_project` reads the real `settings.json` to pick a mode and would otherwise make the result depend on the machine |
| 1c — OpenCode `model` and `permission` survive; a corrupt Gemini settings file errors and is left alone | `opencode_drift_is_quiet_when_the_user_has_their_own_keys`, `merge_preserves_unrelated_top_level_keys`, `a_malformed_target_config_is_an_error_not_a_clobber` |

The manual runs are still worth doing before release. They exercise the sync
engine and the real library, which the collector-level tests deliberately avoid.

One unrelated observation for whoever picks up the next phase.
`plugins::build::features::tests::create_feature_respects_requested_state` and
`create_feature_defaults_to_backlog` share a `TEST_PROJECT` in the real user
data directory and fail intermittently under parallel execution. It reproduces
on a pristine checkout and is not caused by this work.

---

## Phase 2 — Kilo rebrand

**Depends on:** 1b, 1e. **Effort:** high judgement, because of the migration.

Upstream now: project config is `./kilo.json[c]` or `./.kilo/kilo.json[c]`, MCP
under a top-level `mcp` key in the OpenCode dialect (`type: "local"|"remote"`,
`command` as an array, `environment` rather than `env`). `./.kilocode/` is no
longer read.

- [x] Keep `id() == "kilo"`. It is persisted in `Project.agents` and changing it
      orphans every project
- [x] Update `label()` to drop "Code", update `config_description()`
- [x] Write via `write_opencode_dialect_mcp_config`, targeting `.kilo/kilo.json`
- [x] If a root `kilo.json` or `kilo.jsonc` already exists, write into that
      instead, following the same logic as `opencode.rs:170`
- [x] Treat an unparseable `.jsonc` as `Err`, never a clobber
- [x] `detect_in` — add `kilo.json`, `kilo.jsonc`, `.kilo/`, and keep `.kilocode/`
      as a legacy marker so existing projects stay recognised
- [x] `discover_global_mcp_servers` — add `~/.config/kilo/kilo.json[c]` under the
      `mcp` key
- [x] New `migrate_legacy_kilocode` in `sync/engine.rs`, called beside
      `migrate_legacy_cursorrules` ([engine.rs:189](../../src-tauri/src/sync/engine.rs)),
      gated on `project.agents` containing `"kilo"`
- [x] Migration: read `.kilocode/mcp.json` with the existing
      `discover_mcp_servers_from_json(path, "mcpServers", identity)`. Delete the
      file if every server it names is already selected. Otherwise leave it and
      log a warning naming those servers
- [x] Migration: `fs::remove_dir` on `.kilocode/`, not `remove_dir_all`
- [x] Tests: writes `.kilo/kilo.json`; merge preserves `model`; prefers an
      existing root `kilo.json`; migration deletes when servers are known and
      preserves when they are not

Ignoring an existing root `kilo.json` would reproduce the Warp shadowing bug.
Never silently discard a server the user added by hand.

Deferred: `.kilo/agents` and `.kilo/rules`. The directories are documented, the
file formats are not, and `AGENTS.md` already works.

### Phase 2 status

Landed. 787 tests pass (up from 779 at the end of Phase 1), `cargo clippy
--all-targets -- -D warnings` is clean, `make check` is clean.

Two decisions made beyond the checklist, both confirmed with the user before
implementation:

- **No `$schema` field.** Kilo's own docs (`kilo.ai/docs/automate/mcp/using-in-kilo-code`)
  show no `$schema` example, unlike OpenCode's published schema. Rather than
  guess a URL or reuse OpenCode's, `write_opencode_dialect_mcp_config`'s
  `schema_url` parameter became `Option<&str>`; Kilo passes `None`, OpenCode
  keeps passing `Some(OPENCODE_SCHEMA_URL)`.
- **`detect_global_install` fixed too**, though it wasn't in the original
  checklist. It still checked for the VS Code app / `code` CLI, a leftover
  from before the CLI-first rebrand. Now checks `cli_available("kilo")` or
  the presence of `~/.config/kilo/kilo.json[c]`.

Also worth recording: `opencode.rs:170`, cited above as the model for the
root-file-preference logic, turned out not to contain that logic — it's
inside unrelated cache-cleanup code. No such logic existed anywhere in the
codebase before this phase; it was designed fresh for `kilo_code.rs`'s
`resolve_config_path`, modelled on the existing `detect_in` multi-candidate
pattern instead.

One unresolved discrepancy, not acted on: `kilo.ai/docs/code-with-ai/platforms/cli`
mentions a "legacy `./opencode.json[c]`" fallback that the earlier audit
document says Kilo no longer reads. This phase does not read `opencode.json`
for Kilo either way, consistent with the checklist as written.

Manual end-to-end verification (seed `.kilocode/mcp.json`, sync, confirm
`.kilo/kilo.json` appears and `.kilocode/` migrates or is preserved with a
warning; then add a root `kilo.json` and confirm the next sync targets it)
was not run — it needs a registered project and a real sync, the same
constraint noted at the end of Phase 1.

---

## Phase 3 — Move the hook event catalogue into Rust

**Blocks:** Phase 5, so no fifth hand-maintained event array is ever written.
**Effort:** moderate. Touches the frontend.

- [x] Add `fn hook_events(&self) -> &'static [&'static str] { &[] }` to the trait
- [x] Move `CURSOR_SUPPORTED_EVENTS` (`cursor.rs:301`) behind it
- [x] Move `CODEX_SUPPORTED_EVENTS` (`codex_cli.rs:489`) behind it
- [x] Extend Codex to 11 events: add `SessionEnd`, `PreCompact`, `PostCompact`,
      `SubagentStart`, `SubagentStop`
- [x] Add Claude's 30 events, including the missing `MessageDisplay`
- [x] Add `hook_events` to the JSON. It is a
      string list, so it does not belong in `AgentCapabilities`, which is a bool
      struct
- [x] Delete `CLAUDE_CODE_EVENTS`, `CODEX_CLI_EVENTS`, `CURSOR_EVENTS` and
      `EVENTS_BY_AGENT` from
      [Hooks.tsx](../../src/pages/workspace/Hooks.tsx)
- [x] Build the event map from the agent list the page already fetches
- [x] Fix `DEFAULT_EDITOR.event = "SessionStart"` (`Hooks.tsx:138`), which is not
      a valid event for Cursor or Gemini
- [x] Test: `hook_events_and_the_hooks_capability_agree`
- [x] Test: `every_declared_hook_event_survives_a_sync` — build one hook per
      declared event, sync, assert the written config mentions it

Leave Claude Code unfiltered. `sync_claude_code_hooks` does no event filtering
today and should keep not doing it. `hook_events()` is advisory for the picker.
Only agents that already filter should filter. Otherwise a user who knows about a
new Claude event before we do loses their hook silently.

### Phase 3 status

Landed. 793 tests pass (up from 787 at the end of Phase 2), `cargo clippy
--all-targets -- -D warnings` is clean, `make check` is clean.

Implementation notes, and one deviation from the checklist:

- `hook_events` was added to the `AgentInfo` struct in `agent/mod.rs` (the
  DTO returned by the `list_agents` Tauri command, which is what
  `Hooks.tsx` actually calls) rather than to the `list_agents_with_projects`
  json!() macro the checklist cited at `commands/agents.rs:45-53`. That
  command feeds the Providers/Library/Dashboard pages, not the Hooks page,
  and by the time this phase landed the cited line numbers had drifted from
  Phase 1/2 edits. `AgentInfo::from_agent` now populates `hook_events` from
  `agent.hook_events()`, and the frontend `AgentInfo` interface in
  `AgentSelector.tsx` carries a matching optional field.
- Two extra tests beyond the checklist, added because a checklist test that
  only asserts non-emptiness would not have caught the actual audit findings
  (Codex stuck at 6 events, Claude missing `MessageDisplay`): `claude_code.rs`
  asserts `hook_events().len() == 30` and contains `"MessageDisplay"`;
  `codex_cli.rs` asserts `hook_events().len() == 11` and contains all five
  newly-added events. A further `agent_info_serialises_hook_events_as_a_plain_string_array`
  in `agent/mod.rs`'s own test module pins the exact JSON shape
  (`hook_events` as a plain string array, present and empty rather than
  absent for non-hook agents) that the frontend indexes unconditionally.
- `codex_hook_sync_skips_unsupported_events`, an existing test, used
  `PreCompact` as its example of a Codex-unsupported event. Since `PreCompact`
  is now one of the 11 supported events, the test was repointed at `Setup`
  (Claude-only, still unsupported by Codex) and a new
  `codex_hook_sync_accepts_the_five_newly_added_events` test was added
  confirming the five previously-dropped events now reach `.codex/hooks.json`.

Not run: live verification that the Hooks page's Event dropdown actually
repopulates when switching the Target agent selector, because this is a
Tauri desktop app and no tool in this environment can drive or screenshot its
native window (the Browser tools drive a plain browser tab, which has no
Tauri IPC bridge for `invoke("list_agents")` to work against). In its place:
`tsc --noEmit` passes on the rewritten `Hooks.tsx`, and the Rust-side tests
above pin the exact data (11 Codex events, 30 Claude events including
`MessageDisplay`, correct JSON key and shape) that the page's `eventsByAgent`
memo consumes. The manual check from the Verification section below —
opening the Hooks page and confirming the picker behaviour visually — is
still worth doing before release.

---

## Phase 4 — Extract the shared hooks writer

**Depends on:** Phase 3. **Effort:** high judgement. Pure refactor, and the only
acceptance criterion is that the existing hook tests pass unchanged.

Across Claude, Codex, Copilot, Droid and Gemini, six behaviours are identical.
Group by event then matcher into a `BTreeMap` for deterministic output. Append
into an existing matcher group rather than duplicating it. Write script bodies
before the config that references them. Prune empty groups and events. Remove the
file or the `hooks` key when the managed set is empty. Run
`cleanup_managed_hook_scripts` against a keep-list.

Copying that five times is roughly 1,400 lines of near-duplicate code. There are
two flavours, not five. Claude and Gemini merge into a shared settings file and
must preserve user handlers. Codex, Copilot and Droid own their file outright.

- [x] Add `HookWriteSpec` to `agent/mod.rs` with fields `supported_events`,
      `scripts_dir`, `script_command` (renders the command string for a Script
      handler from its filename, so each vendor keeps its own portable prefix),
      `handler` (builds one handler object, where vendors add or rename fields),
      and `group_extras` (extra keys on every matcher group)
- [x] Add `write_owned_hooks_file(hooks_file, hooks, spec)` for Codex, Copilot and
      Droid
- [x] Add `merge_hooks_into_json_settings(settings_path, settings_key, hooks, spec)`
      for Claude Code and Gemini
- [x] Migrate `sync_claude_code_hooks` ([claude_code.rs](../../src-tauri/src/agent/claude_code.rs))
      onto it
- [x] Migrate `sync_codex_hooks` ([codex_cli.rs](../../src-tauri/src/agent/codex_cli.rs))
      onto it
- [x] Confirm the existing hook tests pass unchanged

Reuse `write_managed_hook_script` ([agent/mod.rs](../../src-tauri/src/agent/mod.rs))
and `cleanup_managed_hook_scripts` as they are. Note the former does not
create its directory, so the caller must.

Cursor stays out. It has a sidecar manifest at
`.automatic/state/cursor-hooks.json` (`cursor.rs:325`) and camelCase events. The
helper serves 5 of 6.

**Abandon gate:** if expressing the Claude and Codex writers through these two
entry points needs any `if agent_id == …` branch inside `mod.rs`, stop and copy
per agent instead.

### Phase 4 status

Landed. 793 tests pass — the same count as at the end of Phase 3, which is the
point: this phase added no new tests and removed none, per the checklist's own
acceptance criterion. Every one of Claude Code's 9 hook tests and Codex CLI's
9 hook tests (including the two counting tests added in Phase 3) passes
unchanged. `cargo clippy --all-targets -- -D warnings` is clean, `make check`
is clean, and `cargo fmt --check` is clean on the three touched files (the
repo carries pre-existing formatting drift elsewhere that this phase left
alone, consistent with not touching unrelated code).

The abandon gate was not triggered — no `if agent_id == …` branch was needed
in `mod.rs`. Implementation notes:

- **The managed-handler tags moved, not just the writer.** `HOOK_MANAGED_KEY`
  / `HOOK_MANAGED_VALUE` / `HOOK_ID_KEY` lived in `claude_code.rs`; they are
  now `agent/mod.rs` internals, applied automatically by
  `merge_hooks_into_json_settings` (via a `build_tagged_handler` wrapper
  around whatever `spec.handler` returns) rather than something each
  vendor's `handler` closure has to remember to do itself. Codex's
  `write_owned_hooks_file` path never tags — an owned file is fully
  regenerated every sync, so there's nothing to distinguish from
  user-authored content on the next pass.
- **`supported_events` is asymmetric by design.** `write_owned_hooks_file`
  filters against it and warns on skip (generically — no vendor name in the
  message, since no test checks the wording and Copilot/Droid will share it
  in Phase 5). `merge_hooks_into_json_settings` ignores it entirely, so
  Claude Code stays unfiltered per Phase 3's instruction even though its spec
  populates the field with its real `hook_events()` list — that list is true
  information about the vendor, just not enforced on this path.
  Codex's `codex_hook_sync_skips_unsupported_events` test was already
  repointed at `Setup` in Phase 3 (`PreCompact` became supported); it still
  passes here unchanged, confirming the filter survived the move intact.
- **One behavioural asymmetry preserved deliberately, not by accident.**
  `write_owned_hooks_file` only creates its containing directory once it
  knows there's at least one usable hook, and deletes the file outright when
  there are none — matching Codex's existing "Automatic owns this file, an
  empty one is pointless" behaviour. `merge_hooks_into_json_settings` always
  creates its directory and always writes, even for zero hooks, because the
  settings file may carry keys this agent doesn't own (Claude's `model`,
  `permissions`, …) and empties the `hooks` key rather than assuming it's
  safe to leave the file alone. This is exactly the "remove the file or the
  `hooks` key" split the checklist called for, not a bug in either writer.
- **`group_extras` and the `handler`/`script_command` split are unused by
  both migrated vendors today.** Both Claude and Codex currently need the
  same handler shape (`standard_command_handler`: `type`, `command`,
  optional `timeout`) and no group-level extras (`no_group_extras`). They're
  in the spec now because Phase 5a's Gemini writer needs `group_extras` for
  the optional `sequential` key the audit found in Gemini's hook shape — this
  is the one piece of Phase 4 that has no user yet, included because the very
  next phase depends on it existing, not speculatively.
- **`read_mergeable_json_object`** (the shared helper from Phase 1c/1e)
  replaced Claude's own hand-rolled settings-file read. Behaviourally
  identical — same empty-file and empty-string handling, same
  must-be-an-object requirement — only the error wording changed slightly,
  which no test asserts on.

---

## Phase 5 — New hook integrations

**Depends on:** Phases 3 and 4. **Effort:** repetitive once the shared writer
exists. Cheapest first.

### 5a. Gemini CLI

- [ ] Merge a `hooks` key into `.gemini/settings.json`
- [ ] 11 events: `BeforeTool`, `AfterTool`, `BeforeAgent`, `AfterAgent`,
      `BeforeModel`, `BeforeToolSelection`, `AfterModel`, `SessionStart`,
      `SessionEnd`, `Notification`, `PreCompress`
- [ ] Convert `timeout_sec * 1000`. Gemini's `timeout` is milliseconds while
      `core::Hook::timeout_sec` is seconds. Leave the core model alone
- [ ] Omit `sequential` when optional, so drift has fewer bytes to disagree about
- [ ] Set `hooks: true`
- [ ] Test: idempotent across repeats; `mcpServers` survives; millisecond
      conversion; malformed settings returns `Err`

`.gemini/settings.json` is the same file `write_mcp_config` merges into. Ordering
is safe. Hooks run at `engine.rs:263` and MCP at `:560`, and both
read-modify-write.

### 5b. GitHub Copilot

- [ ] Write one Automatic-owned file, `.github/hooks/automatic.json`, rather than
      one file per hook
- [ ] 8 events: `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
      `PreCompact`, `SubagentStart`, `SubagentStop`, `Stop`. All are a strict
      subset of Claude's names
- [ ] Add `.github/hooks` to the existing `managed_gitignore_paths` override
      (`github_copilot.rs:70`)
- [ ] Set `hooks: true`
- [ ] Test: owned file is removed when the hook set is empty

Document the known overlap. VS Code also reads `.claude/settings.json`, so in a
project with both `claude` and `copilot` selected a Claude-targeted hook also
fires under Copilot. That is the vendor's compatibility read, not something to
work around.

### 5c. Droid

- [ ] `.factory/hooks.json` for config, `.factory/hooks/` for scripts
- [ ] 9 events: `PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `Notification`,
      `Stop`, `SubagentStop`, `PreCompact`, `SessionStart`, `SessionEnd`
- [ ] Explicitly delete a legacy `.factory/hooks/hooks.json` if present.
      `cleanup_managed_hook_scripts` will not touch it, as it lacks the
      `managed-by-automatic` marker
- [ ] Emit `matcher` only. Droid's optional `commandRegex` has no field in
      `core::Hook`, and extending the core model is out of scope
- [ ] Set `hooks: true`
- [ ] Test: legacy `hooks/hooks.json` is removed

---

## Phase 6 — Hook drift collector

**Depends on:** Phase 5. **Effort:** moderate.

`drift.rs` has `collect_mcp_drift`, `collect_skills_drift`,
`collect_commands_drift` and `collect_agents_drift`, but nothing for hooks. After
Phase 5, six agents write hooks into five files, three of which are shared user
files.

- [ ] Owned files (Codex, Copilot, Droid): whole-file byte compare
- [ ] Merged files (Claude, Gemini): extract only handlers tagged
      `_managedBy: "automatic"` and compare that subset

The tempdir trick does not work for merge writers, for the same reason as 1b. This
is a further argument for Phase 4 emitting the ownership tag uniformly across both
merge writers.

---

## Phase 7 — Sub-agent plumbing, then the three new surfaces

**Depends on:** Phase 1. **7a blocks 7b.** **Effort:** 7a needs judgement, 7b to
7d are repetitive.

### 7a. Fix the filename and cleanup machinery

Two latent defects block Copilot.

- [ ] Add an `agent_file_name` trait method, mirroring the existing
      `command_file_name`
- [ ] Route the three inline `format!("{}.{}", machine_name, ext)` call sites
      through it: [helpers.rs:524](../../src-tauri/src/sync/helpers.rs),
      `helpers.rs:589`, and [drift.rs:961](../../src-tauri/src/sync/drift.rs)
- [ ] Replace the stale sweep at `helpers.rs:600-612` with a full-filename
      expected set plus a content marker, following
      `is_managed_command_file` ([agent/mod.rs:781](../../src-tauri/src/agent/mod.rs))
      and `cleanup_stale_managed_command_files` (`:1025`)
- [ ] Marker-gate `cleanup_custom_agents` (`helpers.rs:535`)
- [ ] Test: `sub_agent_filenames_round_trip` — `agent_file_name(n)` starts with
      `n` and ends with `.{agents_file_ext()}`

The stale sweep keys off `path.file_stem()`, which for `foo.agent.md` yields
`foo.agent` and fails `is_valid_agent_machine_name`, so Copilot's stale files
would never be removed. And `cleanup_custom_agents` deletes every `*.md` in the
directory, which is tolerable for `.claude/agents/` and destructive for
`.github/agents/`.

### 7b. GitHub Copilot sub-agents

- [ ] `agents_dir` → `.github/agents`
- [ ] `agent_file_name` → `{name}.agent.md`
- [ ] `agents: true`
- [ ] Test: a hand-written `.github/agents/mine.agent.md` survives the stale sweep
- [ ] Test: it also survives removing Copilot from the project

### 7c. Droid sub-agents

- [ ] `agents_dir` → `.factory/droids`
- [ ] `agents: true`

Markdown with YAML frontmatter (`name`, `description`, `model`, `tools`,
`reasoningEffort`, `mcpServers`). The body is the system prompt and must not be
empty. Close enough to the canonical format that the default pass-through
`convert_agent_content` works. Only the directory name differs.

### 7d. Kiro sub-agents

- [ ] `agents_dir` → `.kiro/agents`
- [ ] `agents_file_ext` → `"json"`
- [ ] `convert_agent_content` turning canonical Markdown and frontmatter into
      Kiro's JSON (`name`, `description`, `prompt` from the body, `mcpServers`,
      `tools`, `model`)
- [ ] `agents: true`
- [ ] Test: conversion produces the right JSON and the filename stem is the id

`codex_cli.rs:271` is the model for a non-Markdown converter.

---

## Phase 8 — Instruction-file migrations

**Depends on:** Phase 1. **Effort:** high judgement. This is the only phase that
deletes users' instruction files.

Generalise `migrate_legacy_cursorrules`
([engine.rs:299-388](../../src-tauri/src/sync/engine.rs)). Its three-branch logic
and its bookkeeping are right. Only the filenames are Cursor-specific.

- [ ] Add a `LegacyInstructionMigration` spec with `agent_id`, `legacy`, `current`
      and `legacy_shadows_current`
- [ ] Add `migrate_legacy_instruction_file(effective_dir, project, spec)` returning
      an optional conflict
- [ ] Route the conflict through the channel
      `collect_instruction_file_conflicts` ([drift.rs:334](../../src-tauri/src/sync/drift.rs))
      already feeds
- [ ] Re-express the Cursor migration through the generalised helper

`legacy_shadows_current` is the one thing Cursor did not need. For Cursor,
`AGENTS.md` outranks `.cursorrules`, so the "both differ" branch could safely log
and move on. For Zed, `.rules` is first in precedence, and for Warp, `WARP.md`
beats `AGENTS.md`. In both cases that branch means the user's sync is a no-op, and
it must reach the UI.

### 8a. Zed

- [ ] `project_file_name()` from `.rules` (`zed.rs:29`) to `AGENTS.md`
- [ ] Keep `.rules` in `detect_in` (`zed.rs:36`) as a legacy marker
- [ ] Confirm the `.rules` entry is removed from
      `project.instruction_file_hashes`
- [ ] Confirm the `.rules` snapshot under `.automatic/snapshots/` is renamed
- [ ] Test all three branches: legacy empty, target empty, both non-empty and
      different

Because `instruction_targets` dedups by filename (`engine.rs:645`), this collapses
the double-write with Codex and Cursor automatically.

### 8b. Warp

- [ ] Migrate `WARP.md` to `AGENTS.md` through the generalised helper
- [ ] Test all three branches

`WARP.md` was never Automatic-written, so `read_project_file` returns the whole
file as user content. There is no hash entry and no snapshot, and the helper
handles all three as no-ops. One consequence to log: a project detected only via
`WARP.md` (`warp.rs:58`) stops autodetecting as Warp once the file is gone. That
is acceptable, since the agent is already in `project.agents`, but it belongs in
the migration log line.

---

## Phase 9 — Enhancements and doc drift

**Depends on:** nothing. Can be picked up at any time. **Effort:** mechanical.

- [ ] `discover_global_mcp_servers` for Antigravity
      (`~/.gemini/config/mcp_config.json`)
- [ ] `discover_global_mcp_servers` for Cline (`~/.cline/mcp.json`)
- [ ] Codex `auth = "oauth"` in `write_mcp_config`. `cursor_auth_block`
      ([cursor.rs:234](../../src-tauri/src/agent/cursor.rs)) is the precedent
- [ ] `antigravity.rs` header: global skills are `~/.gemini/config/skills/`, not
      `~/.gemini/antigravity/skills/`
- [ ] `antigravity.rs` header: close the `mcp_config.json` TODO, note the
      Antigravity CLI shares the harness
- [ ] Source-comment URLs, including the `Hooks.tsx:20-22` header

| Old | Current |
|---|---|
| `docs.claude.com/en/docs/claude-code/*` | `code.claude.com/docs/en/*` |
| `developers.openai.com/codex/*` | `learn.chatgpt.com/docs/*` |
| `kilocode.ai/docs/*` | `kilo.ai/docs/*` |
| `jetbrains.com/help/junie/*` | `junie.jetbrains.com/docs/*` |
| `block.github.io/goose/*` | `goose-docs.ai/docs/*` |

- [ ] Correct finding #2 in the audit document. It overstates the Junie bug.
      Skills do reach `.junie/skills` via the engine's
      `symlink_skills_from_project` step. The real defect is that `.junie/skills`
      is never drift-checked. Fix the finding and its severity
- [ ] Mark each audit finding resolved as the phases land

---

## Deferred, deliberately

- **`automatic-cli`.** The same 16 files need the same changes. Separate work.
- **Kiro's `.kiro/hooks/`.** File-system event automations, structurally unlike
  the lifecycle model `core::Hook` encodes. Needs a design decision, not a port.
- **Pi's `.pi/prompts/`.** Depends on which extension is installed.
- **Antigravity `AGENTS.md` versus `GEMINI.md`.** Our comment says community
  testing found `GEMINI.md` only. Google's API docs now mention `AGENTS.md`.
  Changing `project_file_name` on a guess breaks every Antigravity project
  silently. Needs a first-hand test. The doc-comment fix and global discovery
  still ship in Phase 9.
- **Kilo `.kilo/agents` and `.kilo/rules`.** Formats unverified.
- **Making `collect_mcp_drift` recursive.** Do the targeted seeding in 1b only.
- **Extending `core::Hook` with `commandRegex`** for Droid.
- **Claude Code hooks in skill and agent frontmatter, and plugin
  `hooks/hooks.json`.** A new surface, not a fix.
- **Remaining Codex and Kiro MCP keys**: `env_http_headers`, `required`,
  `enabled_tools`, `disabled_tools`, and Kiro's `oauth`, `autoApprove`,
  `disabledTools`. None broken today.
- **A Codex TOML importer**, which would remove the `agent.id() == "codex"` skip
  at `mcp_format_tests.rs:164`.
- **New agent implementations**: Copilot CLI, Junie CLI and Kiro CLI are distinct
  products from their IDE counterparts. Separate scoping question.

---

## Verification

Per phase, before moving on:

```bash
cd src-tauri && cargo test && cargo clippy -- -D warnings
```

```bash
make check
```

Phase-specific checks:

- **1a** — Sync a test project with Junie selected, then run drift and confirm it
  is clean. Repeat with `sync_mode` set to both `symlink` and `copy` in
  `~/.agents/config.json`.
- **1c** — Put `{"model":"x","permission":{}}` in a `test-projects/*/opencode.json`,
  sync, confirm both keys survive and drift is clean. Corrupt
  `.gemini/settings.json` to `{ not json`, sync, confirm the sync errors and the
  file is untouched.
- **2** — On a project with a pre-existing `.kilocode/mcp.json`, sync and confirm
  `.kilo/kilo.json` appears in the right dialect, `.kilocode/` is gone when its
  servers were all known, and preserved with a warning when one was not. Then add
  a root `kilo.json` and confirm the next sync writes into it.
- **3** — Open the Hooks page. Confirm the event picker populates per agent from
  the backend, that Codex offers 11 events, that Claude offers `MessageDisplay`,
  and that the default event is valid for the selected agent.
- **4** — The existing Claude and Codex hook tests pass unchanged. That is the
  whole acceptance criterion.
- **5** — For each of Gemini, Copilot and Droid: attach a command hook and a
  script hook, sync twice, confirm the config is byte-identical between runs and
  the script is executable with the managed marker. Detach and confirm the managed
  entries and scripts are gone while user-authored content survives. For Gemini,
  confirm `mcpServers` still exists in `.gemini/settings.json` afterwards.
- **7** — Hand-write `.github/agents/mine.agent.md`, sync, confirm it survives.
  Remove Copilot from the project and confirm it still survives.
- **8** — For Zed and Warp, exercise all three migration branches on separate test
  projects. The "both non-empty and different" case must surface a conflict in the
  UI rather than silently succeeding.
- **9** — `automatic_read_project` and the Agents settings page show the corrected
  labels, descriptions and capability badges.

End to end, on a scratch directory under `test-projects/`: select every agent,
sync, and confirm each agent's documented config path exists with the right shape
and that drift reports clean immediately afterwards.
