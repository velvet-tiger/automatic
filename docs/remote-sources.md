# Remote Sources

Remote Sources let Automatic load resources — skills, MCP servers, rules, templates, commands, and agents — from a git repository. Users can install a source once and keep it updated, or use a one-click install link from a README or website.

## How It Works

1. A repository publishes an `automatic.json` manifest at its root.
2. A user installs the source (via the UI, a Tauri command, or a deep link).
3. Automatic clones the repo to `~/.automatic/sources/{owner}/{repo}/`.
4. Resources are unpacked to their canonical locations (skills to `~/.automatic/library/skills/`, rules to `~/.automatic/rules/`, etc.).
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
| `pinning` | No | Ignored by the installer. Kept for manifests that set it (see below). |
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

**Automatic ignores the manifest's `pinning` field.** A repository cannot choose the ref it is tracked at. The pin is the ref the user fetched.

```json
"pinning": {
  "strategy": "branch",
  "ref": "main"
}
```

| Strategy | Behaviour |
|----------|-----------|
| `branch` | Tracks a branch. `update` pulls latest. Default if omitted. |
| `tag` | Pins to a git tag. An update clones the tag again. |
| `commit` | Pins to a specific SHA. An update clones the commit again. |

The install reads the pin from the clone: a commit when the user fetched one by its SHA (even if a tag names it too), else the branch the clone is on, else a tag that names its commit, else the commit. A fetch with no ref clones branch `main`. A `ref` of 40 hex characters is fetched as a commit; any other `ref` is passed to `git clone --branch`.

Every ref is checked before git sees it. A commit must be 7 to 40 hex digits. A branch or tag must follow the `git check-ref-format` rules and may not start with `-`, so it can never be read as a git option. Refs are passed after `--end-of-options` or `--`. An update refuses a stored pin that fails the check, as older versions could store one taken from the manifest.

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

### Name and path rules

Automatic checks every name and path in a manifest before it uses them. These rules apply to skills, collections and `skill_json` as well.

A `name` must be a single file name. It cannot be empty, `.` or `..`. It cannot contain `/`, `\`, `:` or a control character, and it cannot end with a dot or a space.

A `path` must be relative and must stay inside the folder that holds the manifest. It cannot be absolute, contain `..` or use `\`. A symlink is followed only when its target is inside that folder. The manifest file itself cannot be a symlink to a file outside it.

A resource that breaks a rule is skipped, and the install result carries a warning that names it. The other resources still install. A `skill_json` path that breaks a rule fails the whole fetch.

An MCP server config is saved the same way as a server added in the app, so its `env` values are stored encrypted. An `env` value that starts with `enc:v1:` is stored empty.

### Expected file formats per type

| Type | Source format | Installed to | Notes |
|------|-------------|--------------|-------|
| Skills | Directory with `SKILL.md` | `~/.automatic/library/skills/{name}/` | Companion files (scripts/, docs/, etc.) are included. |
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

## Install Links
Users can install a source with a single click using the hosted install URL on `tryautomatic.app`. The website constructs the underlying `automatic://install?...` deep link and routes the user into the app:

```
<a href="https://tryautomatic.app/install?repo=acme/ai-toolkit">https://tryautomatic.app/install?repo=acme/ai-toolkit</a>
<a href="https://tryautomatic.app/install?repo=acme/ai-toolkit&ref=v2.0.0">https://tryautomatic.app/install?repo=acme/ai-toolkit&ref=v2.0.0</a>
<a href="https://tryautomatic.app/install?repo=acme/monorepo&dir=packages/ai-config">https://tryautomatic.app/install?repo=acme/monorepo&dir=packages/ai-config</a>
<a href="https://tryautomatic.app/install?repo=acme/monorepo&dir=packages/ai-config&ref=v2.0.0">https://tryautomatic.app/install?repo=acme/monorepo&dir=packages/ai-config&ref=v2.0.0</a>
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `repo` | Yes | GitHub `owner/repo` format. A value that is not an owner and a repository name is refused. |
| `ref` | No | Git ref to pin to (tag, SHA, or branch). |
| `dir` | No | Subdirectory within the repo where `automatic.json` lives (monorepo support). |

When clicked, the website hands off to Automatic, which fetches the manifest and shows a confirmation dialog before installing anything.

If you need the raw deep link for testing or app internals, Automatic still handles `automatic://install?...` URIs directly.

### Monorepo Support

When `automatic.json` lives in a subdirectory (e.g. `packages/ai-config/automatic.json`), pass the `dir` parameter. All resource paths in the manifest are resolved relative to that subdirectory, not the repo root.

```
<a href="https://tryautomatic.app/install?repo=acme/monorepo&dir=packages/ai-config">https://tryautomatic.app/install?repo=acme/monorepo&dir=packages/ai-config</a>
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
<a href="https://tryautomatic.app/install?repo=aurabx/skills"><img src="https://tryautomatic.app/badges/install.svg" alt="Install in Automatic"></a>
```

**HTML (local badge from this repo):**

```html
<a href="https://tryautomatic.app/install?repo=your-org/your-repo">
  <img src="docs/assets/install-in-automatic.svg" alt="Install in Automatic" height="32">
</a>
```

**HTML (with a pinned version):**

```html
<a href="https://tryautomatic.app/install?repo=your-org/your-repo&ref=v2.0.0">
  <img src="https://tryautomatic.app/badges/install.svg" alt="Install in Automatic" height="32">
</a>
```

**HTML dark variant (for dark backgrounds):**

```html
<a href="https://tryautomatic.app/install?repo=your-org/your-repo">
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
- **Tag/commit-pinned sources**: the clone is deleted and re-created at the stored ref. A shallow clone at a tag or commit is on a detached HEAD, which `git pull` cannot fast-forward.

On update, the manifest is re-parsed with its `skill_json` resolved. Then:

- **Reinstalled**: each resource the source installed before, still owns, and the new manifest still lists.
- **Not installed**: a resource the new manifest adds, or one the user did not select. These are returned in `new_available`. An install with a selection adds them.
- **Left alone**: a resource the user has since saved as their own, or another source took over.
- **Kept on failure**: a resource whose reinstall fails keeps its installed copy and its provenance. The failure is a warning. A skill is copied into a hidden staging folder in `skills/` first, and the installed folder is swapped only once the copy is complete.
- **Env values kept**: when an MCP server the source owns is reinstalled, each env value the user filled in is kept for every key the new config declares.
- **Collections**: the collections the source installed before are reinstalled. New ones are listed in `new_available`.
- **Removed**: a resource the manifest no longer lists, and only while this source still owns it.

## Conflict Resolution

A source may replace or delete only the entries whose provenance names that source. Names are compared without regard to letter case or Unicode form (NFKC, lowercased), because APFS and NTFS treat such names as one file.

When a selected resource matches an existing library entry:

1. **Same source owns it, same name** — replaced (this is an update).
2. **Different source owns it** — a conflict the user may confirm.
3. **No provenance record (user-created)** — a "local" conflict the user may confirm.
4. **Built-in, bundled, plugin-owned, required or Codex entry** — blocked. It can never be replaced. Bundled includes the default rules and the bundled sub-agents.
5. **Name differs only in letter case or form** — blocked. The write would land in the other entry's file.
6. **A skill in another skill folder** (`~/.agents/skills`, `~/.claude/skills`, or an agent's own folder) — a "local" conflict the user may confirm, as the library copy would shadow it. Removing the source later deletes only the library copy. Projects, profiles and the skill's own records keep the name, because it still resolves to the user's skill.

Collections follow the same rules, keyed `collection:{slug}`. A bundled collection is blocked. A collection in Discover that no source lists is local. A collections file that does not parse fails the collection rather than being overwritten.

An install with any unconfirmed conflict installs nothing and returns the conflicts. The install dialog calls `check_source_conflicts` after the fetch. A resource with a conflict starts unselected. Selecting it confirms the replacement; a group's "select all" never does. Every write that is not a confirmed or owned replacement is create-only, so it fails rather than replace a file. A replacement never writes through a symlink.

Provenance for an install is written once at the end, then the source is registered. If either write fails, the entries the install created are deleted again. Entries it replaced keep the new content and their old ownership, and the error names them.

### User writes

Every user save, import and delete clears the source's provenance for that name: the editors (skills, MCP servers, rules, templates, sub-agents, commands), the MCP rule tools, skill imports from skills.sh, a repository, a local folder, a package or an agent package, and every delete command. The user owns the name from then on, and a resource made later under it is not the source's. If the write fails, the provenance is put back.

One exception: an MCP server save that changes only env values keeps the source's ownership, so filling in an API key does not take the server over.

A source's own install and a cloud pull do not clear provenance. Provenance lives in `~/.automatic/source-provenance.json` on each machine and is not synced. An edit or delete on one machine therefore does not release the source's claim on another machine. There, an update of the source can still replace a version the user pulled from the cloud, and a removal can delete it.

## Tauri Commands

| Command | Description |
|---------|-------------|
| `fetch_remote_source(repo, git_ref?, dir?)` | Clone repo and return parsed manifest as JSON. `dir` for monorepo subdirectory. |
| `install_remote_source(repo, selected?, dir?, overrides?)` | Install resources from a fetched source. `dir` must match fetch. `overrides` lists the `type:name` key of each confirmed conflict. |
| `update_remote_source(repo)` | Bring the clone up to date and reinstall what the source owns. Uses stored `directory`. |
| `remove_remote_source(repo)` | Remove source and all resources it provided. |
| `list_remote_sources()` | List all registered sources. |
| `check_source_conflicts(repo, dir?, selected?)` | Pre-flight conflict check. `dir` for monorepo subdirectory. Without `selected`, every resource is checked. |
| `handle_install_uri(uri)` | Parse an `automatic://install?...` URI. Returns `repo`, `ref`, `dir`. |
