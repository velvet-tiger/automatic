---
name: automatic-remote-source-authoring
description: Create and maintain Automatic remote source packages — automatic.json manifests, skill.json references, collections, badges, and the full directory structure for publishing resources via git repositories.
authors:
  - Automatic
---

# Authoring Automatic Remote Source Packages

A remote source is a git repository that publishes resources (skills, MCP servers, rules, templates, commands, agents) for installation into Automatic. This skill guides you through creating, validating, and maintaining these packages.

## When to Use This Skill

Use this skill when the user wants to:

- Create an `automatic.json` manifest for a new repository
- Add resources (skills, rules, MCP servers, etc.) to an existing source package
- Set up a `skill.json` file alongside or instead of `automatic.json`
- Configure per-agent overrides for resources
- Add an "Install in Automatic" badge to a README
- Create or update marketplace collections
- Troubleshoot manifest validation issues

## Manifest: `automatic.json`

The manifest lives at the root of a git repository. All resource paths are relative to it.

**JSON Schema:** `https://tryautomatic.app/schemas/automatic.json`

### Minimal Example

The smallest valid manifest — a single skill:

```json
{
  "$schema": "https://tryautomatic.app/schemas/automatic.json",
  "name": "my-toolkit",
  "version": "1.0.0",
  "description": "One-line summary of what this source provides",
  "skills": {
    "entries": [
      {
        "name": "my-skill",
        "path": "./skills/my-skill",
        "description": "What this skill does"
      }
    ]
  }
}
```

### Full Example

A manifest using every available section:

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

  "pinning": { "strategy": "branch", "ref": "main" },

  "collections": [
    { "path": "./collections/acme-frontend.json" }
  ],

  "skills": {
    "skill_json": "./skill.json",
    "entries": [
      {
        "name": "acme-deploy",
        "path": "./skills/acme-deploy",
        "description": "Deployment automation for Acme infrastructure",
        "entrypoint": "SKILL.md",
        "tags": ["devops", "deployment"],
        "category": "infrastructure"
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
    { "name": "acme-project", "path": "./templates/acme-project.json", "description": "Standard project scaffold" }
  ],

  "commands": [
    { "name": "acme-deploy", "path": "./commands/acme-deploy.md", "description": "Deploy to staging" }
  ],

  "agents": [
    { "name": "acme-reviewer", "path": "./agents/acme-reviewer.md", "description": "Code review agent" }
  ],

  "agent_overrides": {
    "_defaults": {
      "skills": ["acme-react-patterns", "acme-deploy"],
      "mcp_servers": ["acme-db"],
      "rules": ["acme-code-style"],
      "commands": ["acme-deploy"],
      "agents": ["acme-reviewer"]
    },
    "claude": { "include_rules": ["acme-claude-hints"] },
    "cursor": { "exclude_mcp_servers": ["acme-db"] }
  }
}
```

## Required Fields

Only three fields are required at the top level:

| Field | Type | Constraint |
|-------|------|------------|
| `name` | string | Lowercase, hyphens allowed. Pattern: `^[a-z][a-z0-9-]*$` |
| `version` | string | Semver. Pattern: `^\d+\.\d+\.\d+` |
| `description` | string | One-line, max 256 characters |

Everything else is optional. A source can provide just skills, just rules, or any combination.

## Resource Types

Every resource section (except `skills`) uses the same shape:

```json
{ "name": "machine-name", "path": "./relative/path", "description": "What it does" }
```

### Expected File Formats

| Type | Source file format | Installed to | Notes |
|------|-------------------|--------------|-------|
| **Skills** | Directory with `SKILL.md` | `~/.agents/skills/{name}/` | Include companion dirs: `scripts/`, `docs/`, `references/`, `examples/` |
| **MCP Servers** | `.json` | `~/.automatic/mcp_servers/{name}.json` | Standard MCP server config |
| **Rules** | `.md` (plain markdown) | `~/.automatic/rules/{name}.json` | Installer wraps content in `{"name": ..., "content": ...}` |
| **Templates** | `.json` (ProjectTemplate) | `~/.automatic/project_templates/{name}.json` | Must match Automatic's ProjectTemplate struct |
| **Commands** | `.md` | `~/.automatic/commands/{name}.md` | Markdown command definition |
| **Agents** | `.md` | `~/.automatic/agents/{name}.md` | Markdown sub-agent definition |

### Writing a Skill

Create a directory with at least a `SKILL.md`:

```
skills/my-skill/
├── SKILL.md          # Required: main instruction file
├── scripts/          # Optional: helper scripts
├── references/       # Optional: reference material
├── docs/             # Optional: extended documentation
├── examples/         # Optional: code examples
└── templates/        # Optional: file templates
```

The `SKILL.md` should have YAML frontmatter:

```markdown
---
name: my-skill
description: What this skill does and when to use it.
authors:
  - Your Name
---

# My Skill

Instructions for the agent...
```

### Writing a Rule

Rules are plain markdown files. The `name` field in the manifest becomes the display name; the filename stem becomes the machine name.

```markdown
# Acme Code Style

## TypeScript

- Strict mode enabled. No `any` types.
- Use `interface` for object shapes, `type` for unions.
...
```

### Writing an MCP Server Config

Standard MCP server JSON with `command`, `args`, and optional `env`:

```json
{
  "command": "npx",
  "args": ["-y", "@acme/mcp-db-server"],
  "env": {
    "ACME_DB_URL": "",
    "ACME_DB_READONLY": "true"
  }
}
```

### Writing a Template

Templates must match Automatic's `ProjectTemplate` JSON structure:

```json
{
  "name": "My Project Template",
  "description": "What this template sets up",
  "skills": ["skill-name-1", "skill-name-2"],
  "mcp_servers": ["server-name"],
  "providers": [],
  "agents": ["claude"],
  "project_files": [],
  "unified_instruction": "",
  "unified_rules": ["rule-name", "automatic-service"],
  "user_agents": [],
  "user_commands": []
}
```

### Writing a Command

Commands are markdown files with instructions the agent follows when the command is invoked:

```markdown
# Deploy

1. Verify the working tree is clean: `git status --porcelain`
2. Run the test suite: `npm run test:ci`
3. Build the project: `npm run build`
4. Deploy: `acme deploy --env staging`
5. Verify: `acme status --env staging`
```

### Writing a Sub-Agent

Agents are markdown files defining a specialised persona:

```markdown
# Code Reviewer

You are a code review sub-agent. For every change, verify:

1. **Type safety** — No `any` types, all functions have return types.
2. **Error handling** — API calls handle errors explicitly.
3. **Tests** — New logic has corresponding tests.
...
```

## Skills Section

The `skills` field supports two modes:

```json
"skills": {
  "skill_json": "./skill.json",
  "entries": [...]
}
```

- **`skill_json`** — path to a [skill.json](https://github.com/velvet-tiger/skill.json) spec file. All skills in it are imported.
- **`entries`** — inline skill definitions (same shape as skill.json entries).

Either can be omitted. If both are present and a name appears in both, `entries` wins.

### skill.json Only (No automatic.json Needed)

Repositories that only provide skills can use a bare `skill.json` at root — no `automatic.json` required:

```json
{
  "$schema": "https://skills.json.org/schema/1.0.0/skills.schema.json",
  "name": "my-skills",
  "version": "1.0.0",
  "description": "My skill package",
  "skills": [
    {
      "name": "my-skill",
      "path": "./skills/my-skill",
      "description": "What it does"
    }
  ]
}
```

## Pinning

Controls how Automatic tracks the git ref:

```json
"pinning": { "strategy": "branch", "ref": "main" }
```

| Strategy | Behaviour |
|----------|-----------|
| `branch` | Tracks a branch. Users can pull updates. Default if omitted. |
| `tag` | Pins to a tag. Fixed until user changes the pin. |
| `commit` | Pins to a SHA. Fixed until user changes the pin. |

## Collections

Collections group resources in Automatic's Marketplace UI. Reference JSON files in the repo:

```json
"collections": [
  { "path": "./collections/my-collection.json" }
]
```

Each file follows Automatic's marketplace collection format:

```json
{
  "id": "your-org/collection-name",
  "name": "My Collection",
  "slug": "my-collection",
  "description": "What this collection provides",
  "author": {
    "type": "provider",
    "name": "Your Org",
    "url": "https://your-org.dev"
  },
  "icon": "your-org.dev",
  "tags": ["tag1", "tag2"],
  "skills": [
    {
      "name": "my-skill",
      "display_name": "My Skill",
      "description": "What it does",
      "source": "your-org/your-repo",
      "id": "your-org/your-repo/my-skill",
      "kind": "github"
    }
  ],
  "mcp_servers": [],
  "templates": []
}
```

## Agent Overrides

Control which resources are enabled per agent tool:

```json
"agent_overrides": {
  "_defaults": {
    "skills": ["skill-a", "skill-b"],
    "rules": ["rule-a"]
  },
  "claude": {
    "include_rules": ["claude-specific-rule"]
  },
  "cursor": {
    "exclude_skills": ["skill-b"]
  }
}
```

- **`_defaults`** — resources enabled for all agents.
- **Per-agent keys** — use `include_*` to add and `exclude_*` to remove.
- **Effective set** = `defaults + includes - excludes`.
- **Agent IDs**: `claude`, `cursor`, `codex`, `kilo-code`, `cline`, `kiro`, `gemini-cli`, `goose`, `opencode`, `warp`, `zed`, `junie`, `droid`, `antigravity`, `github-copilot`.

## Install Badge

Add a one-click install button to your README:

```markdown
[![Install in Automatic](https://tryautomatic.app/badges/install.svg)](automatic://install?repo=your-org/your-repo)
```

With a pinned version:

```markdown
[![Install in Automatic](https://tryautomatic.app/badges/install.svg)](automatic://install?repo=your-org/your-repo&ref=v2.0.0)
```

HTML for websites:

```html
<a href="automatic://install?repo=your-org/your-repo">
  <img src="https://tryautomatic.app/badges/install.svg" alt="Install in Automatic" height="32">
</a>
```

## Recommended Directory Layout

```
your-repo/
├── automatic.json                   # Manifest (required)
├── skill.json                       # Optional: skill.json spec
├── skills/
│   ├── skill-one/
│   │   ├── SKILL.md
│   │   └── references/
│   └── skill-two/
│       └── SKILL.md
├── mcp-servers/
│   └── my-server.json
├── rules/
│   ├── code-style.md
│   └── claude-hints.md
├── templates/
│   └── my-project.json
├── commands/
│   └── deploy.md
├── agents/
│   └── reviewer.md
└── collections/
    └── my-collection.json
```

## Validation Checklist

Before publishing, verify:

1. `automatic.json` is valid JSON and passes the schema (`$schema` field gives editor validation).
2. Every `path` in the manifest points to a file or directory that exists.
3. Skill directories contain at least the entrypoint file (default: `SKILL.md`).
4. MCP server and template files are valid JSON.
5. Rule, command, and agent files are non-empty markdown.
6. Collection `slug` values are unique and URL-safe.
7. Names in `agent_overrides` match names declared in the resource sections.
8. If using `skill_json`, the referenced file exists and is valid.

## Workflow: Creating a New Source Package

1. Create the repository and add `automatic.json` with `name`, `version`, `description`.
2. Add resource directories and files for each type you want to publish.
3. Fill in the resource sections in `automatic.json` pointing to each file.
4. Optionally add `agent_overrides` if some resources are agent-specific.
5. Optionally create a collection JSON and reference it in `collections`.
6. Add the install badge to your README.
7. Commit and push. Users can now install via `automatic://install?repo=owner/repo`.
