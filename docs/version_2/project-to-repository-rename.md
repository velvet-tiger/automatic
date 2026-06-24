# 2.0 — Rename `Project` → `Repository`

*Status: proposed for the 2.0 release. Unshipped — design of record.*

## Context

Today's `Project` is the heavyweight per-directory configuration object: it owns a working
directory plus the skills, agents, MCP servers, rules, sync mode, drift state, memory, and
template/group associations that sync into that one directory. Everything it holds is
*repository-scoped* — it all targets a single tree on disk.

The intended model evolution is two-step:

1. **2.0 (this work):** rename the existing concept `Project` → `Repository`. The current
   object maps 1:1 onto "a repository" (a local directory today; a remote git repo later).
2. **Later (out of scope):** introduce a *new* `Project` type as a parent container that
   wraps multiple `Repository` objects and holds parent-level data.

This rename is therefore not cosmetic — it repositions the existing concept to its correct
place in a future hierarchy and frees the name `Project` (and the on-disk `projects/`
directory) for the future parent. Exploration confirmed `ProjectGroup` is an orthogonal
tagging/folder mechanism (not the seed of the future parent), so it is renamed alongside.

### Decisions locked in

- **MCP tools: hard rename.** Rename `automatic_*_project` → `automatic_*_repository`
  outright. No alias/deprecation layer. External agents calling the old names break on
  upgrade — this is an accepted, release-note-worthy breaking change for 2.0. Bundled
  skills/rules that call the tools MUST be updated in the same change.
- **Analytics: leave event names as-is.** The 8 Amplitude `project_*` events keep their
  names (external identifiers, not domain code). Add a clarifying comment at the divergence;
  make no other analytics change.
- **On-disk paths: migrate now.** Ship an idempotent startup migrator that renames
  `~/.automatic/projects/` → `~/.automatic/repositories/` and `.automatic/project.json` →
  `.automatic/repository.json`, with legacy read-through for one release.

### Explicitly out of scope

- The future parent `Project` type (additive, separate effort).
- Remote-repository support. The "a Repository may be a remote git repo" idea requires
  relaxing the deeply-baked "exactly one local `directory`" invariant (~171 read sites of
  `.directory`); that is a later, separate change. `Repository.directory` stays required.
- Renaming serde **field** names that hit disk (e.g. `ProjectTemplate.project_files`). The
  struct has no `#[serde(rename_all)]`, so field names *are* the on-disk keys; none contain
  "project" except `project_files`, which we keep as-is to avoid a schema migration.

## Naming map

| Current | New |
|---|---|
| `Project` (struct, `core/types.rs:278`) | `Repository` |
| `ProjectGroup` (`core/types.rs:462`) | `RepositoryGroup` (field `projects` → `repositories`) |
| `ProjectMode`, `ProjectRef`, `ProjectContext` | `RepositoryMode`, `RepositoryRef`, `RepositoryContext` |
| `ProjectTemplate` (`core/templates.rs:26`) | `RepositoryTemplate` (keep serde field `project_files`) |
| `ProjectProblem` / `ProjectProblemsReport` (`sync/drift.rs`) | `RepositoryProblem` / `RepositoryProblemsReport` |
| `get_projects_dir()` (`core/paths.rs:72`) | `get_repositories_dir()` (returns `projects/` until the migrator lands) |
| modules `core/projects.rs`, `core/project_files.rs`, `commands/projects.rs`, `commands/project_files.rs`, `cli/projects.rs` | `*repositories*` / `*repository_files*` |
| 18 Tauri commands `*_project*` | `*_repository*` (+ matching invoke strings) |
| 5 MCP tools `automatic_*_project` | `automatic_*_repository` (+ param field `project` → `repository`) |
| storage keys `automatic.projects.{selected,order}` | `automatic.repositories.{selected,order}` |

Amplitude `project_*` events: **unchanged**.

## Execution mechanics

**Compiler-driven, not grep-driven.** Rename each Rust type at its definition, run
`cargo build`, and fix the call sites the compiler flags. Do **not** use a
`pub type Project = Repository` bridge — the single-binary crate has no external consumer
needing it, and a lingering alias lets stale references rot. One type at a time, compile
green between each.

**Scripted vs manual rule:** script only identifiers the compiler/`tsc` will verify (type
names, fn names, `use` paths, module file renames). Hand-edit anything crossing a boundary
the compiler can't see: doc comments and ~40 user-visible strings ("project" as English vs
domain term), the MCP tool-name string literals and instructions block, and the on-disk
serde field names (which stay). Exclude `analytics.ts` event strings from any scripted pass.

**Tauri commands are internal** (frontend-only, 121 invoke sites) — rename backend +
frontend atomically in one change, no alias. The invoke **argument keys** are part of the
contract too. The failure is silent, so verify with:

```
# stale project-named invoke strings remaining in the frontend:
rg -n 'invoke[<(][^)]*"[a-z_]*project[a-z_]*"' src/
# every TS invoke string has a backing handler in generate_handler!:
comm -23 <(rg -ohN '"[a-z_]+"' src --glob '*.ts*' | tr -d '"' | sort -u) \
         <(rg -oN '^\s+\K[a-z_]+,' src-tauri/src/lib.rs | tr -d ', ' | sort -u)
```

## On-disk migrator

Reuse the existing pattern `migrate_top_level_to_library` at
[paths.rs:112](../../src-tauri/src/core/paths.rs) — same idempotency contract (skip if
legacy absent, merge if target exists, `fs::rename` with `copy_dir_recursive` cross-FS
fallback, return migrated list). Add `migrate_projects_to_repositories()` doing:

1. `~/.automatic/projects/` → `~/.automatic/repositories/` (reuse the dir rename/merge block).
2. Inside each repo working dir: `.automatic/project.json` → `.automatic/repository.json`
   (single-file rename, guarded by `.exists()`).
3. Memory dir (`~/.automatic/memory/{name}.json`) is keyed by repo name, not by the word
   "project" — verify it is a no-op.
4. **Legacy read-through for one release:** `get_repositories_dir()` and the per-repo JSON
   reader fall back to the old path if the new one is absent (covers not-yet-restarted and
   downgrade cases).

Invoke from the Tauri `.setup()` closure in [lib.rs](../../src-tauri/src/lib.rs) —
immediately after the existing `migrate_top_level_to_library()` call and **before** the
first `core::list_*` call, mirroring the existing `match … eprintln!` logging block.

## PR phasing

Smallest independently-shippable, individually-reversible sequence. Order 1→5.

1. **PR-1 — Rust internal rename.** Types, modules, fns, `get_projects_dir` →
   `get_repositories_dir` (still returns `projects/`). MCP tool-name strings and Tauri
   command names **unchanged** here. Purely compiler-gated; no public or disk change.
2. **PR-2 — Tauri commands + frontend (atomic).** Rename 18 commands, `generate_handler!`,
   121 invoke sites + arg keys, 18 TS interfaces, storage keys (with a one-time localStorage
   copy-old→new migrator), user-visible strings. Analytics events left as-is + comment.
3. **PR-3 — MCP public hard rename (BREAKING).** Rename 5 tool-name strings + param fields
   `project` → `repository`, update the `mcp.rs` instructions block, and update **in
   lockstep** all bundled skills/rules/templates that call the tools
   ([assets/rules/automatic/automatic-service.md](../../src-tauri/assets/rules/automatic/automatic-service.md),
   [assets/skills/automatic-features/SKILL.md](../../src-tauri/assets/skills/automatic-features/SKILL.md),
   the `automatic` skill), plus root [CLAUDE.md](../../CLAUDE.md), [AGENTS.md](../../AGENTS.md),
   [README.md](../../README.md). Flag as a breaking change in the 2.0 release notes.
4. **PR-4 — On-disk migrator (⚠ DATA; ship ALONE).** `migrate_projects_to_repositories()`
   + point `get_repositories_dir()` at `repositories/` + legacy read-through + full
   migration test suite. Only PR that mutates user disk state; release in its own version.
5. **PR-5 — Cleanup (later release).** Remove the legacy read-through fallbacks once the
   grace window has passed.

## Verification

- **Per PR:** `make check` (tsc + vite + `cargo check`) is the merge gate — it catches every
  renamed Rust call site and TS interface/invoke type. Then `cargo test` (`make test`).
- **Rust tests:** colocated `#[cfg(test)]` modules in `core/projects.rs`, `sync/drift.rs`,
  `sync/engine.rs`. For the migrator, clone the `migration_tests` style at
  [paths.rs:236](../../src-tauri/src/core/paths.rs) using the `with_test_home` sandbox:
  empty-home no-op, rename, idempotent re-run, legacy-fallback read.
- **Frontend:** `src/test/renderProjects.tsx` + `src/test/tauriMock.ts` +
  `Projects.smoke.test.tsx` drive invoke through the mock — a renamed string that doesn't
  match a handler fails here.
- **End-to-end (PR-4):** seed a legacy `~/.automatic-dev/projects/x.json`, run migration,
  then exercise `read_repository("x")` + a sync + a memory read and assert all succeed.
- **MCP (PR-3):** start the server in `mcp-serve` mode and confirm the renamed tools list
  and respond; confirm bundled skills no longer reference `automatic_*_project`
  (`rg -n 'automatic_[a-z_]*project' src-tauri/assets`).
