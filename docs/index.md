# Automatic — Documentation Index

Reference and design documentation for the Automatic desktop app. Source-of-truth project instructions live in [CLAUDE.md](../CLAUDE.md) and [AGENTS.md](../AGENTS.md); this directory holds longer-form references.

## UI and design

- [Workspace UI Design Rules](./ui-design.md) — quiet chrome rules for project overview, sidebar, Summary, and project editor header/tabs.

## Systems

- [Plugin System](./plugins.md) — plugin architecture, registry, and lifecycle.
- **Assets** — bundled skills, rules, templates, and how they are loaded. Reference lives in the sibling `automatic-meta` repo at `general/assets.md`.
- [Content Library](./library.md) — the external `automatic-library` repository, how the app extracts and refreshes it, manifest schema, retired assets, and the signing model.
- [Remote Sources](./remote-sources.md) — loading resources from git repositories via `automatic.json` manifests.
- **Sub-Agents Feature** — Automatic's sub-agent model and storage. Reference lives in the sibling `automatic-meta` repo at `general/sub-agents/sub-agents.md`.
- [Session Tracking](./session-tracking.md) — design for tracking agent sessions across projects.
- **Cloud Library Sync** — bidirectional sync of `~/.automatic/` to `tryautomatic.app`. Design docs (webapp contract, client state, multi-library plan, and the archived v1 plan) live in the sibling `automatic-meta` repo under `general/plans/cloud-sync/` and `general/archive/cloud-sync/`.

## Operations

- [CLI](./cli.md) — the `automatic` command-line interface shipped in the desktop binary.
- [Code Signing](./code-signing.md) — `automatic-dev` certificate setup for local builds.
- [Test Coverage](./test-coverage.md) — current coverage analysis and gaps.
- [Featured Community Items](./featured-community.md) — curating the Community > Featured page.

## Agent Reference

- [Upstream Audit — 2026-07-30](./agents/upstream-audit-2026-07-30.md) — every supported agent checked against its vendor's current documentation, with the resulting gap list.
- [Agent Gap Remediation Plan](./agents/agent-gap-remediation-plan.md) — phased checklist closing the audit findings, with dependencies so the work can be split up.
- **Per-agent format reference** — lives in the sibling `automatic-meta` repo at `general/agents/`. Start at that repo's `INDEX.md` → "Agent reference"; the index there lists every supported agent (Claude Code, Codex CLI, Cursor, Gemini CLI, GitHub Copilot, Cline, Kilo, Kiro, Junie, Goose, Warp, Antigravity, OpenCode, Droid, Pi, Z Code, Zed). The code in `src-tauri/src/agent/*.rs` is the source of truth; the meta reference is kept in sync with it.

## Version 2.0

- [Rename `Project` → `Repository`](./version_2/project-to-repository-rename.md) — repositions the existing per-directory config object as `Repository`, freeing `Project` for a future parent container. Includes the on-disk migrator and a breaking MCP tool rename.

## Plans (in flight / proposed)

Cross-repo plans live in `automatic-meta` under `general/plans/`. Notable in-flight material there includes the cloud-sync contract, the webapp library rebuild specification, and the projects sync contract. See that repo's `INDEX.md` for the current list.

## Examples

- **Remote source repository example** — a complete example git repository publishing skills, rules, templates, commands, agents, and MCP servers via `automatic.json`. Lives in the sibling `automatic-meta` repo at `general/remote-sources-example/`.
