# Content Library

The Automatic app ships a set of built-in skills, rules, instructions, and subagents. Those assets live in a separate repository, [`velvet-tiger/automatic-library`](https://github.com/velvet-tiger/automatic-library), so content updates can ship without a new app release.

This document is the design and operational reference for that library from the app's side. The library's own `README.md` is public-facing; its `CLAUDE.md` is the contributor guide.

## Status

- **Phase 1 landed.** The library ships as a git submodule at `automatic-app/automatic-library/`, packed into a `.zip` by `src-tauri/build.rs`, and read at runtime by `src-tauri/src/core/bundled_library.rs`. The four asset kinds (skills, rules, instructions, subagents) load from that archive rather than from `include_str!` of `src-tauri/assets/`.
- **Phase 2 landed.** `Settings.library_version` tracks default-install state against `bundled_library::version()`. Retired rules move via `retired.json` in the library repo. `Settings.bundled_skills_version` is retained for legacy JSON round-trip only.
- **Phase 3a landed.** The bootstrap version comparison is semver-aware — the app never rolls back an installed library that is newer than the binary snapshot. Tauri commands `get_library_version` and `check_library_updates` are wired; the latter polls `api.github.com/repos/velvet-tiger/automatic-library/releases/latest`.
- **Phase 4 landed (pipeline only).** The library repo now ships `.github/workflows/release.yml` (tag → verify → zip → minisign → publish) and `.github/workflows/pr.yml` (manifest parity gate). A maintainer still needs to generate the signing keypair per `automatic-library/KEYGEN.md` and commit `src-tauri/keys/library.pub` here.
- **Phase 5 landed.** Redundant copies under `src-tauri/assets/{rules,instructions,subagents}/` and library-superseded skill directories under `src-tauri/assets/skills/` are removed. The `common-docs` rule content moved into `src-tauri/src/plugins/common_docs/documentation.md` where the plugin owns it. What remains in `src-tauri/assets/` is app-owned: the nine skills bundled through `bundled_app_skills`, the `automatic-service` rule wired through `APP_BUNDLED_RULES`, discover/language metadata, and provider assets.
- **Phase 3b (download / verify / apply / background scheduler)** is not yet implemented. Unblocked once the first signed release lands on the library repo.

App-side residue that stays in the binary (not in the library):

- `bundled_app_skills` — Automatic-specific skills (`automatic`, `automatic-features`) and third-party vendored skills (Laravel, PHP, Python, Tailwind CSS, Terraform, Vercel/React, Laravel Pennant).
- `rules.rs` `APP_BUNDLED_RULES` — the `automatic-service` rule, which describes the Automatic MCP surface and belongs with the product.
- `rules.rs` `LIBRARY_RULE_DISPLAY_NAMES` — display-name mapping for library rules, so the library can ship content while the app owns UI copy.

## Scope

In the library:

- **Skills** — `skills/{id}/` directories.
- **Rules** — `rules/{pack}/{name}.md`.
- **Instructions** — `instructions/{name}.md`.
- **Subagents** — `subagents/{pack}/{name}.md`.
- **Hooks** — `hooks/{name}.json`. See the security section below.

Not in the library:

- Discover surfaces (`assets/discover/`), agent format adapters (`src-tauri/src/agent/*.rs`), provider metadata, curated MCP-server registry, and language modules. Those stay in the app repository because they are code or code-adjacent registries.

## How the app consumes the library (Phase 1)

1. **Submodule.** `automatic-library/` is a git submodule at the app repo root, pinned to a specific library commit. `git submodule update --init` fetches it. CI adds `submodules: true` to `actions/checkout`.
2. **Build-time pack.** `src-tauri/build.rs` walks the submodule, compresses it into `${OUT_DIR}/library.zip`, and writes the semver from `automatic-library/VERSION` into `${OUT_DIR}/library_version.txt`. `cargo:rerun-if-changed` markers cover the tree.
3. **Runtime read.** `src-tauri/src/core/bundled_library.rs` embeds the archive via `include_bytes!` and lazily extracts every entry into an in-memory `HashMap` on first access. `manifest.json` and `retired.json` are deserialised once and cached in `OnceLock`s.
4. **Loader wiring.** `install_default_skills_inner`, `install_default_rules_inner`, `install_default_instructions_inner`, and `install_default_subagents_inner` iterate typed views from `bundled_library` (`skills()`, `rules()`, `instructions()`, `subagents()`) alongside any app-only entries. Each writes into `~/.automatic/library/{skills,rules,instructions}/` and `~/.automatic/agents/` exactly as it did before.
5. **Manifest is source of truth.** The app reads `manifest.json` for what the library contains. Content silently added to the tree without a manifest update is invisible to the app.

## How the app will consume the library (Phase 3, not yet implemented)

- On a schedule and on user demand from Settings, the app polls the library repo's GitHub Releases for a newer version than `bundled_library::version()`.
- When a newer release is available, the app downloads `library-vX.Y.Z.zip` and `library-vX.Y.Z.zip.minisig`, verifies the signature with `rsign2` against a public key baked into the binary, and rehashes every file against the archive's own `manifest.json`.
- If verification succeeds, the loaders are re-run with `force: true` pointing at the newly-extracted archive as content source. `~/.automatic/library/…` is refreshed in place.
- If any check fails, the on-disk library is untouched and the app keeps its bundled version until the next scheduled attempt.

## Runtime layout

Everything stays under the existing `<root>/library/` tree. The library location is not moved; only the source of the bytes that seed it changes.

```
~/.automatic/                # ~/.automatic-dev/ in debug builds
  settings.json              # tracks default-install state
  library/
    skills/…                 # written by install_default_skills_inner
    rules/…                  # written by install_default_rules_inner
    instructions/…           # written by install_default_instructions_inner
  agents/…                   # written by install_default_subagents_inner
```

User edits are preserved across upgrades and (once Phase 3 lands) library refreshes: the installers use `force: false` on every run except the "Reinstall Defaults" reset path.

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

1. Introduce a `BundledLibrary` module in `src-tauri/src/core/`. Owns extraction of the embedded snapshot to `~/.automatic/library/` on first run, and owns reading `manifest.json`.
2. Replace the `include_str!` list in `bundled_skills.rs` and equivalents with a call into `BundledLibrary` that reads from `current/` at runtime.
3. Add a build step that packs `automatic-library/` at a pinned version into a single `include_bytes!` blob. Adding a new asset to the library no longer requires a Rust edit.
4. Add the background refresh loop, signature verification, hash verification, atomic swap, and rollback.
5. Move `REMOVED_DEFAULT_RULES` from the binary into `retired.json` on the library side. Delete the constant from the app.
6. Drop the `{pack}-{filename}` concatenation from the rules sync path. In its place, expect library source filenames to carry the pack prefix already (`rules/automatic/automatic-guardrails.md`) and install them verbatim. This is a coordinated change across the library and the app: the library rename and the loader change must land together, otherwise every installed rule file gets double-prefixed.

Open questions before step 3:

- **Signing tool.** Minisign vs cosign.
- **Build-time acquisition.** Git submodule vs cloning a pinned tag in a build script. Submodule is more reproducible; script keeps the app repo lighter.
