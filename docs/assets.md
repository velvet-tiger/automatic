# Assets

**Date:** 2026-04-17
**Status:** Active

---

## Overview

An **Asset** in Automatic is any reusable, portable unit of agent configuration that the hub stores, validates, and syncs into agent tools. Assets share common handling: they can be bundled with the app, installed from remote sources, stored in the user's library, assigned to projects, and written to agent tool directories through the sync engine. Text-bearing assets are run through the security scanner (`scan_text_asset_report`) before being written to disk.

"Asset" is a conceptual umbrella — it is not a single Rust type. The security-scan taxonomy `AssetKind` in `core/asset_security.rs` covers the text-bearing subset; the broader asset concept described here includes configuration-only and composite types as well.

---

## Asset Types

### Skills

Prompt-based capabilities that teach an agent a specific workflow or domain.

- **Shape** — a directory containing a `SKILL.md` (instructions + YAML frontmatter) plus optional companion files.
- **Sub-components treated as assets:**
  - **Skill Manifest** — the `skill.json` (or `automatic.json` `skills` section) entry describing the skill's identity, name, description, and metadata.
  - **Companion Files** — scripts, templates, reference docs, or any other file inside the skill directory that the skill may load at runtime.
- **Source locations** — bundled in `src-tauri/assets/skills/`, installed to the user library, or scoped to an individual project.
- **Sync** — written to agent tool skill directories as either a symlink or a copy (controlled by the global sync mode).

### Instructions

Standalone instruction files that guide agent behaviour at the project level.

- **Shape** — a single markdown file.
- **Usage** — assigned to a project and either injected into the project's main instruction file (`CLAUDE.md`, `AGENTS.md`, etc.) or written as individual files under `.automatic/instructions/` when `instructions_index_mode` is enabled.
- **Distinguishing trait** — unlike rules, instructions are free-form project guidance rather than categorised behavioural constraints.

### Rules

Reusable behavioural constraints such as code style, review checklists, debugging methodology, or security guardrails.

- **Shape** — a single markdown file.
- **Source locations** — bundled in `src-tauri/assets/rules/`, imported from remote sources, or authored locally.
- **Sync** — injected inline into agent instruction files or emitted as discrete files under `.automatic/instructions/` (index mode), depending on project configuration.

### Templates

Reusable markdown templates intended as starting points for repeatable deliverables (e.g., Agent Project Brief, Session Context).

- **Shape** — a single markdown file with placeholder sections.
- **Source locations** — bundled in `src-tauri/assets/templates/` or imported from remote sources.
- **Usage** — available to agents through the library; referenced by agents when producing structured output.

Not to be confused with **Project Templates** (see below).

### Sub-Agents

Specialised agent role definitions — pre-configured personas for focused tasks (reviewer, debugger, planner, etc.).

- **Shape** — a single markdown file with YAML frontmatter describing the sub-agent's role, tools, and behaviour.
- **Source locations** — bundled in `src-tauri/assets/subagents/` or authored in the library.
- **Sync** — written to agent tool sub-agent directories where supported (primarily Claude Code).

### Commands

Reusable, named command definitions that a project or agent can invoke as a shorthand for a workflow.

- **Shape** — a single markdown file describing the command and its procedure.
- **Source locations** — library (global) or project-scoped.
- **Sync** — written to agent tool command directories, or referenced from `.agents/commands-index.md` when repo-local.

### MCP Servers

Model Context Protocol server configurations that connect agents to external services.

- **Shape** — a JSON configuration entry (command, args, env, transport).
- **Source locations** — curated registry in `src-tauri/assets/discover/featured-mcp-servers.json`, user library, or remote-source manifests.
- **Sync** — merged into each supported agent tool's MCP configuration file.

### Collections

Curated bundles that group related assets (skills, MCP servers, templates, etc.) around a workflow, stack, or domain.

- **Shape** — a JSON file defining the collection's metadata and the set of member assets.
- **Source locations** — bundled in `src-tauri/assets/marketplace/`, or contributed by remote sources through `collections` manifest entries.
- **Usage** — presented in the marketplace; installing a collection installs its constituent assets together.

### Project Templates

Pre-built project configurations that seed a new project with a standard set of instructions, rules, skills, MCP servers, and other assets in one step.

- **Shape** — a JSON file matching the `ProjectTemplate` structure, plus any referenced asset files.
- **Source locations** — library, marketplace, or remote-source manifests.
- **Usage** — applied to a project on creation or on demand to establish a baseline configuration.

---

## Common Lifecycle

All assets broadly follow the same lifecycle:

1. **Authored or published** — bundled with the app, added to the user library, or declared in a remote-source `automatic.json` manifest.
2. **Installed** — text assets pass through `enforce_text_asset` (secret scanning, path validation, symlink rejection) before being written to the user's config directory.
3. **Assigned** — attached to one or more projects.
4. **Synced** — written into agent tool configuration directories, either directly or through the sync engine's drift-detection pipeline.
5. **Drift-checked** — on-disk files are compared against the saved configuration; manual edits surface as drift until re-synced.

---

## Relationship to `AssetKind`

The `AssetKind` enum in `core/asset_security.rs` is a narrower taxonomy used only by the security scanner and currently covers the text-bearing subset: `Skill`, `SkillManifest`, `CompanionFile`, `UserCommand`, `UserAgent`, `Rule`, `Template`. Configuration-only asset types — MCP servers, collections, project templates, and standalone instructions — are part of the broader asset concept but are not enumerated by `AssetKind` today. This document is the conceptual source of truth; the enum is an implementation detail scoped to content scanning.
