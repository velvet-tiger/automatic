# Automatic — Documentation Index

Reference and design documentation for the Automatic desktop app. Source-of-truth project instructions live in [CLAUDE.md](../CLAUDE.md) and [AGENTS.md](../AGENTS.md); this directory holds longer-form references.

## Systems

- [Plugin System](./plugins.md) — plugin architecture, registry, and lifecycle.
- [Assets](./assets.md) — bundled skills, rules, templates, and how they are loaded.
- [Remote Sources](./remote-sources.md) — loading resources from git repositories via `automatic.json` manifests.
- [Sub-Agents Feature](./sub-agents/sub-agents.md) — Automatic's sub-agent model and storage.
- [Session Tracking](./session-tracking.md) — design for tracking agent sessions across projects.
- **Cloud Library Sync** — bidirectional sync of `~/.automatic/` to `tryautomatic.app`.
  - [Webapp Contract](./plans/cloud-sync/contract.md) — endpoints, bundle/response shapes, merge semantics, database schema. *(source of truth)*
  - [Client State & Diff Algorithm](./plans/cloud-sync/client-state.md) — desktop-side state file and reconcile algorithm. *(source of truth)*
  - [Cloud Library Sync — Historical Plan](./plans/cloud-sync/cloud-library-sync.md) — v1 design notes, kept for rationale; superseded by the two docs above.

## Operations

- [CLI](./cli.md) — the `automatic` command-line interface shipped in the desktop binary.
- [Code Signing](./code-signing.md) — `automatic-dev` certificate setup for local builds.
- [Test Coverage](./test-coverage.md) — current coverage analysis and gaps.
- [Featured Community Items](./featured-community.md) — curating the Community > Featured page.

## Agent Reference

- [Agent Reference Index](./agents/README.md) — supported agents, instructions files, MCP config paths, skills and sub-agents directories.
  - [Claude Code](./agents/claude-code.md)
  - [Codex CLI](./agents/codex-cli.md)
  - [Cursor](./agents/cursor.md)
  - [Gemini CLI](./agents/gemini-cli.md)
  - [GitHub Copilot](./agents/github-copilot.md)
  - [Cline](./agents/cline.md)
  - [Kilo Code](./agents/kilo-code.md)
  - [Kiro](./agents/kiro.md)
  - [Junie](./agents/junie.md)
  - [Goose](./agents/goose.md)
  - [Warp](./agents/warp.md)
  - [Antigravity](./agents/antigravity.md)
  - [OpenCode](./agents/opencode.md)
  - [Droid](./agents/droid.md)
- [Sub-Agent Format Reference](./sub-agents/formats.md) — file formats, storage locations, and frontmatter schemas per agent.

## Plans (in flight / proposed)

- [Projects.tsx — Phase 2 Refactor Plan](./projects-phase2-refactor.md) — test-harness-first split of the projects screen.
- [Multi-Agent Support in Settings > Agents](./plans/multi-agent/plan.md) — adding GitHub Models, Cloudflare, Z.ai, OpenCode Zen, OpenAI, and an AI Gateway router.

## Examples

- [Remote Source Repository Example](./remote-sources-example/README.md) — a complete example git repository publishing skills, rules, templates, commands, agents, and MCP servers via `automatic.json`.

## Assets

- [`assets/`](./assets/) — install-in-automatic badge SVGs for use in third-party READMEs.
