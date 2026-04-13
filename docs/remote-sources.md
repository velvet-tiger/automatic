# Remote Sources

Remote Sources let Automatic load resources — skills, MCP servers, rules, templates, commands, and agents — from a git repository. Users can install a source once and keep it updated, or use a one-click `automatic://` link from a README or website.

## How It Works

1. A repository publishes an `automatic.json` manifest at its root.
2. A user installs the source (via the UI, a Tauri command, or a deep link).
3. Automatic clones the repo to `~/.automatic/sources/{owner}/{repo}/`.
4. Resources are unpacked to their canonical locations (skills to `~/.agents/skills/`, rules to `~/.automatic/rules/`, etc.).
5. Provenance is tracked so resources can be updated or cleanly removed.

## Manifest Format (`automatic.json`)

The manifest lives at the root of a git repository. All paths are relative to the manifest file.

The JSON Schema is at [`docs/schemas/automatic.schema.json`](schemas/automatic.schema.json) and will be published at `https://tryautomatic.app/schemas/automatic.json`.

```json
{
  "$schema": "https://tryautomatic.app/schemas/automatic.json",
  "name": "acme-ai-toolkit",
  "version": "2.1.0",
  "description": "Full-stack AI agent configuration for Acme",
  "author": { "name": "Acme Corp", "url": "https://acme.dev" },
  "license": "MIT",
  "repository": { "type": "git", "url": "https://github.com/acme/ai-toolkit" },
  "keywords": ["acme", "react", "fullstack"],

  "pinning": {
    "strategy": "branch",
    "ref": "main"
  },

  "collections": [
    { "path": "./collections/acme-frontend.json" }
  ],

  "skills": {
    "skill_json": "./skill.json",
    "entries": [
      {
        "name": "acme-deploy-skill",
        "path": "./skills/deploy",
        "description": "Deployment automation skill",
        "entrypoint": "SKILL.md",
        "tags": ["devops"]
      }
    ]
  },

  "mcp_servers": [
    { "name": "acme-db", "path": "./mcp-servers/acme-db.json", "description": "Acme database MCP server" }
  ],

  "rules": [
    { "name": "acme-code-style", "path": "./rules/acme-code-style.md", "description": "Acme coding standards" }
  ],

  "templates": [
    { "name": "acme-project", "path": "./templates/acme-project.json", "description": "Standard Acme project scaffold" }
  ],

  "commands": [
    { "name": "acme-deploy", "path": "./commands/acme-deploy.md", "description": "Deploy to Acme staging" }
  ],

  "agents": [
    { "name": "acme-reviewer", "path": "./agents/acme-reviewer.md", "description": "Code review agent" }
  ],

  "agent_overrides": {
    "_defaults": {
      "skills": ["acme-react-patterns", "acme-deploy-skill"],
      "mcp_servers": ["acme-db"],
      "rules": ["acme-code-style"],
      "commands": ["acme-deploy"],
      "agents": ["acme-reviewer"]
    },
    "claude": {
      "include_rules": ["acme-claude-hints"]
    },
    "cursor": {
      "exclude_skills": ["acme-react-patterns"]
    }
  }
}
```

## Top-Level Fields

| Field | Required | Description |
|-------|----------|-------------|
| `$schema` | No | JSON Schema URL for editor validation. |
| `name` | Yes | Package identifier. Lowercase with hyphens. |
| `version` | Yes | Semver version string. |
| `description` | Yes | One-line summary of what this source provides. |
| `author` | No | Object with `name`, optional `url` and `email`. |
| `license` | No | SPDX license identifier (e.g. `"MIT"`, `"Apache-2.0"`). |
| `repository` | No | Object with `type` (`"git"`) and `url`. |
| `homepage` | No | Documentation or landing page URL. |
| `keywords` | No | Array of search/filter terms. |
| `pinning` | No | Version pinning configuration (see below). |
| `collections` | No | Array of collection file references (see below). |
| `skills` | No | Skills section (see below). |
| `mcp_servers` | No | Array of MCP server resource entries. |
| `rules` | No | Array of rule resource entries. |
| `templates` | No | Array of template resource entries. |
| `commands` | No | Array of command resource entries. |
| `agents` | No | Array of agent resource entries. |
| `agent_overrides` | No | Per-agent include/exclude configuration (see below). |

Every resource section is optional. A source can provide just skills, just rules, or any combination.

## Pinning

Controls how Automatic tracks the git ref for this source.

```json
"pinning": {
  "strategy": "branch",
  "ref": "main"
}
```

| Strategy | Behaviour |
|----------|-----------|
| `branch` | Tracks a branch. `update` pulls latest. Default if omitted. |
| `tag` | Pins to a git tag. Only updates when the user changes the pin. |
| `commit` | Pins to a specific SHA. Only updates when the user changes the pin. |

When `pinning` is omitted, defaults to `{ "strategy": "branch", "ref": "main" }`.

## Skills

The `skills` field supports two modes, used together or independently:

```json
"skills": {
  "skill_json": "./skill.json",
  "entries": [
    {
      "name": "acme-deploy-skill",
      "path": "./skills/deploy",
      "description": "Deployment automation",
      "entrypoint": "SKILL.md",
      "tags": ["devops"]
    }
  ]
}
```

### `skill_json`

Path to a [skill.json](https://github.com/velvet-tiger/skill.json) spec file. The installer reads it and imports all skills listed in its `skills` array. This reuses the standard skill.json format and Automatic's existing import machinery.

### `entries`

Inline skill definitions. Each entry follows the same shape as a `skill.json` skill entry:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique skill identifier. Should match directory name. |
| `path` | Yes | Relative path to the skill directory. |
| `description` | Yes | What this skill does. |
| `entrypoint` | No | Main instruction file. Defaults to `"SKILL.md"`. |
| `tags` | No | Search/filter tags. |
| `category` | No | Primary category. |
| `version` | No | Skill-specific version override. |

If both `skill_json` and `entries` are present, skills from both are installed. If a skill name appears in both, the `entries` version wins.

### skill.json Fallback

If a repo has **no** `automatic.json` but has a `skill.json` at root, Automatic treats it as a skills-only source. No manifest authoring is needed for repos that only provide skills via the standard spec.

## Resource Entries

MCP servers, rules, templates, commands, and agents use a uniform shape:

```json
{ "name": "resource-name", "path": "./relative/path", "description": "What it does" }
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Machine name identifier. Used as the filename when installed. |
| `path` | Yes | Relative path to the resource file in the repo. |
| `description` | Yes | Human-readable summary. |

### Expected file formats per type

| Type | Source format | Installed to | Notes |
|------|-------------|--------------|-------|
| Skills | Directory with `SKILL.md` | `~/.agents/skills/{name}/` | Companion files (scripts/, docs/, etc.) are included. |
| MCP Servers | `.json` | `~/.automatic/mcp_servers/{name}.json` | Standard MCP server config JSON. |
| Rules | `.md` | `~/.automatic/rules/{name}.json` | Markdown content is wrapped in `{"name": ..., "content": ...}`. |
| Templates | `.json` | `~/.automatic/project_templates/{name}.json` | Must match the `ProjectTemplate` struct (see example). |
| Commands | `.md` | `~/.automatic/commands/{name}.md` | Markdown command definition. |
| Agents | `.md` | `~/.automatic/agents/{name}.md` | Markdown agent definition. |

## Collections

The `collections` array references JSON files in the repo that follow Automatic's marketplace collection format:

```json
"collections": [
  { "path": "./collections/acme-frontend.json" }
]
```

Each referenced file must contain a valid collection object:

```json
{
  "id": "acme/frontend",
  "name": "Acme Frontend",
  "slug": "acme-frontend",
  "description": "Frontend tools for Acme projects",
  "author": { "type": "provider", "name": "Acme Corp", "url": "https://acme.dev" },
  "icon": "acme.dev",
  "tags": ["frontend", "react"],
  "skills": [
    { "name": "acme-react-patterns", "display_name": "React Patterns", "description": "..." }
  ],
  "mcp_servers": [],
  "templates": []
}
```

On install, collections are appended to `~/.automatic/marketplace/collections.json` and appear in the Marketplace UI. A source can declare zero or many collections.

## Agent Overrides

The `agent_overrides` section controls which resources are enabled for each agent tool.

```json
"agent_overrides": {
  "_defaults": {
    "skills": ["acme-react-patterns"],
    "mcp_servers": ["acme-db"],
    "rules": ["acme-code-style"],
    "commands": ["acme-deploy"],
    "agents": ["acme-reviewer"]
  },
  "claude": {
    "include_rules": ["acme-claude-hints"]
  },
  "cursor": {
    "exclude_skills": ["acme-react-patterns"],
    "exclude_mcp_servers": ["acme-db"]
  }
}
```

### `_defaults`

Lists resources enabled for **all** agents by default. Each field is an array of resource names (must match names declared in the corresponding resource sections).

### Per-agent keys

Keyed by agent ID (`claude`, `cursor`, `codex`, `kilo-code`, etc.). Each supports:

| Field | Effect |
|-------|--------|
| `exclude_skills` | Remove these skills for this agent. |
| `exclude_mcp_servers` | Remove these MCP servers for this agent. |
| `exclude_rules` | Remove these rules for this agent. |
| `exclude_commands` | Remove these commands for this agent. |
| `exclude_agents` | Remove these sub-agents for this agent. |
| `include_skills` | Add these skills for this agent (on top of defaults). |
| `include_mcp_servers` | Add these MCP servers for this agent. |
| `include_rules` | Add these rules for this agent. |
| `include_commands` | Add these commands for this agent. |
| `include_agents` | Add these sub-agents for this agent. |

The effective set for an agent is: `defaults + includes - excludes`.

## Deep Links (`automatic://`)

Users can install a source with a single click using a deep link:

```
<a href="automatic://install?repo=acme/ai-toolkit">automatic://install?repo=acme/ai-toolkit</a>
<a href="automatic://install?repo=acme/ai-toolkit&ref=v2.0.0">automatic://install?repo=acme/ai-toolkit&ref=v2.0.0</a>
<a href="automatic://install?repo=acme/monorepo&dir=packages/ai-config">automatic://install?repo=acme/monorepo&dir=packages/ai-config</a>
<a href="automatic://install?repo=acme/monorepo&dir=packages/ai-config&ref=v2.0.0">automatic://install?repo=acme/monorepo&dir=packages/ai-config&ref=v2.0.0</a>
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `repo` | Yes | GitHub `owner/repo` format. |
| `ref` | No | Git ref to pin to (tag, SHA, or branch). |
| `dir` | No | Subdirectory within the repo where `automatic.json` lives (monorepo support). |

When clicked, the OS opens Automatic, which fetches the manifest and shows a confirmation dialog before installing anything.

### Monorepo Support

When `automatic.json` lives in a subdirectory (e.g. `packages/ai-config/automatic.json`), pass the `dir` parameter. All resource paths in the manifest are resolved relative to that subdirectory, not the repo root.

```
<a href="automatic://install?repo=acme/monorepo&dir=packages/ai-config">automatic://install?repo=acme/monorepo&dir=packages/ai-config</a>
```

The `directory` is stored in `sources.json` so updates resolve from the same subdirectory.

### Adding an Install button to a README

Use the badge SVGs in `docs/assets/`. Replace `your-org/your-repo` with your GitHub `owner/repo`.

**Preview:**

| Light | Dark |
|-------|------|
| ![Install in Automatic](assets/install-in-automatic.svg) | ![Install in Automatic](assets/install-in-automatic-dark.svg) |

**HTML (hosted badge):**

```html
<a href="automatic://install?repo=aurabx/skills"><img src="https://tryautomatic.app/badges/install.svg" alt="Install in Automatic"></a>
```

**HTML (local badge from this repo):**

```html
<a href="automatic://install?repo=your-org/your-repo">
  <img src="docs/assets/install-in-automatic.svg" alt="Install in Automatic" height="32">
</a>
```

**HTML (with a pinned version):**

```html
<a href="automatic://install?repo=your-org/your-repo&ref=v2.0.0">
  <img src="https://tryautomatic.app/badges/install.svg" alt="Install in Automatic" height="32">
</a>
```

**HTML dark variant (for dark backgrounds):**

```html
<a href="automatic://install?repo=your-org/your-repo">
  <img src="https://tryautomatic.app/badges/install-dark.svg" alt="Install in Automatic" height="32">
</a>
```

## Local Storage

### Source cache

Cloned repos are stored at `~/.automatic/sources/{owner}/{repo}/`. These are shallow git clones used as the source of truth for unpacking resources.

### Source registry (`~/.automatic/sources.json`)

Tracks all installed sources:

```json
[
  {
    "repo": "acme/ai-toolkit",
    "name": "Acme AI Toolkit",
    "version": "2.1.0",
    "pin": { "strategy": "branch", "ref": "main" },
    "last_fetched": "2026-04-12T10:00:00Z",
    "commit_sha": "abc123...",
    "resources": {
      "skills": ["acme-react-patterns"],
      "mcp_servers": ["acme-db"],
      "rules": ["acme-code-style"]
    },
    "collection_slugs": ["acme-frontend"]
  }
]
```

### Provenance (`~/.automatic/source-provenance.json`)

Maps each installed resource to the source that provided it:

```json
{
  "skill:acme-react-patterns": "acme/ai-toolkit",
  "mcp_server:acme-db": "acme/ai-toolkit",
  "rule:acme-code-style": "acme/ai-toolkit"
}
```

Used for conflict detection (two sources providing the same resource name) and clean removal.

## Updating a Source

- **Branch-tracking sources**: `update_remote_source` runs `git pull --ff-only`. If fast-forward fails (force push upstream), the clone is deleted and re-created.
- **Tag/commit-pinned sources**: cannot be updated without changing the pin. The user must explicitly set a new ref.

On update, the manifest is re-parsed and resources are diffed against the previous install. Added resources are installed, changed resources are overwritten, and removed resources are deleted.

## Conflict Resolution

When a resource name already exists:

1. **Same source owns it** — overwritten silently (this is an update).
2. **Different source owns it** — conflict is surfaced to the user. Options: replace, skip.
3. **No provenance record (user-created)** — treated as "local". Default: skip with warning.

Conflicts are checked before install begins and presented in the confirmation UI.

## Tauri Commands

| Command | Description |
|---------|-------------|
| `fetch_remote_source(repo, git_ref?, dir?)` | Clone repo and return parsed manifest as JSON. `dir` for monorepo subdirectory. |
| `install_remote_source(repo, selected?, dir?)` | Install resources from a fetched source. `dir` must match fetch. |
| `update_remote_source(repo)` | Pull latest and re-install changed resources. Uses stored `directory`. |
| `remove_remote_source(repo)` | Remove source and all resources it provided. |
| `list_remote_sources()` | List all registered sources. |
| `check_source_conflicts(repo, dir?)` | Pre-flight conflict check. `dir` for monorepo subdirectory. |
| `handle_install_uri(uri)` | Parse an `automatic://install?...` URI. Returns `repo`, `ref`, `dir`. |
