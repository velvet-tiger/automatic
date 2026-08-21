# Content Library

The Automatic app ships a set of built-in skills, rules, instructions, and subagents. Those assets live in a separate repository, [`velvet-tiger/automatic-library`](https://github.com/velvet-tiger/automatic-library), so content updates can ship without a new app release.

This document is the design and operational reference for that library from the app's side. The library's own `README.md` is public-facing; its `CLAUDE.md` is the contributor guide.

## Status

The library repository exists and is populated. The app still compiles its assets in via `include_str!` under `src-tauri/src/core/bundled_skills.rs` and equivalents. Switching the app over to the library is the next step and is described under "Migration" below.

## Scope

In the library:

- **Skills** — `skills/{id}/` directories.
- **Rules** — `rules/{pack}/{name}.md`.
- **Instructions** — `instructions/{name}.md`.
- **Subagents** — `subagents/{pack}/{name}.md`.
- **Hooks** — `hooks/{name}.json`. See the security section below.

Not in the library:

- Discover surfaces (`assets/discover/`), agent format adapters (`src-tauri/src/agent/*.rs`), provider metadata, curated MCP-server registry, and language modules. Those stay in the app repository because they are code or code-adjacent registries.

## How the app consumes the library

1. **Bundled snapshot.** Every Automatic release embeds a snapshot tarball of a pinned library version in the binary. The snapshot is the fallback for offline installs.
2. **First run.** If `~/.agents/library/current/` does not exist, the app extracts the bundled snapshot into it.
3. **Background refresh.** On a schedule (default weekly) and on user demand from Settings, the app polls GitHub Releases for the newest tag in the app's supported major-version range.
4. **Verify and swap.** When a newer version is available, the app downloads `library-vN.tar.gz` and `library-vN.tar.gz.sig` to a temp path, verifies the signature, verifies each file's sha256 against `manifest.json`, extracts to `~/.agents/library/next/`, then atomically renames `next` → `current`. The previous version becomes `previous/` for one-step rollback.
5. **Manifest is source of truth.** The app never walks the extracted tree to discover assets. It reads `manifest.json` and looks up files by the paths and hashes recorded there. This means an asset silently added to the tree without a manifest update is invisible to the app.
6. **`library_version`** is recorded in `~/.agents/config.json` so the app knows what has been extracted without re-scanning.

Failures during download, signature verification, hash verification, or extraction leave `current/` untouched.

## Runtime layout

```
~/.agents/
  config.json                # tracks library_version
  library/
    current/                 # the extracted library, read-only
      manifest.json
      skills/…
      rules/…
      instructions/…
      subagents/…
      hooks/…
    previous/                # last version, for rollback
    next/                    # transient, during download+extract
```

`current/` is a read-only template. User edits fork into the existing `~/.agents/` stores (`~/.agents/skills/`, etc.), matching how skill and rule editing already works today. Upstream refreshes never overwrite user edits.

## Manifest schema

`manifest.json` at the library root. Schema version 1:

```json
{
  "library_version": "0.1.0",
  "manifest_schema": 1,
  "assets": [
    {
      "kind": "skill",
      "id": "automatic-debugging",
      "root": "skills/automatic-debugging",
      "files": [
        { "path": "skills/automatic-debugging/SKILL.md", "sha256": "…" }
      ]
    },
    {
      "kind": "rule",
      "pack": "automatic",
      "id": "guardrails",
      "path": "rules/automatic/guardrails.md",
      "sha256": "…"
    },
    {
      "kind": "instruction",
      "id": "Session Context",
      "path": "instructions/Session Context.md",
      "sha256": "…"
    },
    {
      "kind": "subagent",
      "pack": "automatic",
      "id": "planner",
      "path": "subagents/automatic/planner.md",
      "sha256": "…"
    },
    {
      "kind": "hook",
      "id": "session-start-log",
      "path": "hooks/session-start-log.json",
      "sha256": "…"
    }
  ]
}
```

Kinds are `skill`, `rule`, `instruction`, `subagent`, `hook`, and `manifest-fragment`. A fragment is a loose top-level file like `skills/skill.json` that the library owns but that is not itself an asset the app installs.

## Retired assets

`retired.json` at the library root. Read after every library refresh:

```json
{
  "retired": [
    {
      "kind": "rule",
      "pack": "automatic",
      "id": "example",
      "retired_in": "0.3.0",
      "reason": "Superseded by rules/automatic/replacement.md"
    }
  ]
}
```

When the app finds a retired asset that a project still references, it detaches the reference on next sync. This is the same pattern the current `REMOVED_DEFAULT_RULES` list handles, moved out of the binary and into the library.

Deleting an asset from the tree without adding it to `retired.json` orphans any project reference to it. The app-side loader treats an unexpected absence as an error, not as a silent removal.

## Security

Hooks contain executable content. Every hook merged into the library is code that will run on end-users' machines. The security posture is:

1. **Signed releases only.** The app trusts a library tarball only if its signature verifies against a public key baked into the binary.
2. **Manifest-pinned hashes.** After signature verification, every file is re-hashed and compared to `manifest.json`. A file that does not match is refused, even if the tarball signature is valid.
3. **Human review.** Every pull request to the library repository that adds or modifies a hook requires a maintainer review before merge.
4. **No arbitrary sources.** The app fetches releases only from the pinned library repository. There is no configuration for a third-party library source.

Signing tool is undecided. Minisign is smaller and simpler; cosign is more standard. Default recommendation is minisign.

## Migration plan

The app currently loads assets at compile time. The switch to library-driven loading is a self-contained refactor:

1. Introduce a `BundledLibrary` module in `src-tauri/src/core/`. Owns extraction of the embedded snapshot to `~/.agents/library/current/` on first run, and owns reading `manifest.json`.
2. Replace the `include_str!` list in `bundled_skills.rs` and equivalents with a call into `BundledLibrary` that reads from `current/` at runtime.
3. Add a build step that packs `automatic-library/` at a pinned version into a single `include_bytes!` blob. Adding a new asset to the library no longer requires a Rust edit.
4. Add the background refresh loop, signature verification, hash verification, atomic swap, and rollback.
5. Move `REMOVED_DEFAULT_RULES` from the binary into `retired.json` on the library side. Delete the constant from the app.

Open questions before step 3:

- **Signing tool.** Minisign vs cosign.
- **Build-time acquisition.** Git submodule vs cloning a pinned tag in a build script. Submodule is more reproducible; script keeps the app repo lighter.
