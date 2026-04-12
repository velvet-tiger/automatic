# Example: Remote Source Repository

This directory shows the complete structure of a git repository that publishes resources to Automatic via the Remote Sources system.

## Directory Layout

```
.
├── automatic.json                          # Manifest (required)
├── skill.json                              # skill.json spec (optional, referenced by automatic.json)
│
├── skills/
│   ├── acme-react-patterns/
│   │   └── SKILL.md                        # Skill loaded via skill.json
│   └── acme-deploy/
│       └── SKILL.md                        # Skill declared as inline entry
│
├── mcp-servers/
│   └── acme-db.json                        # MCP server config
│
├── rules/
│   ├── acme-code-style.md                  # Rule (plain markdown)
│   └── acme-claude-hints.md                # Rule (Claude-only via agent_overrides)
│
├── templates/
│   └── acme-fullstack-project.json         # Project template (Automatic JSON format)
│
├── commands/
│   └── acme-deploy.md                      # Command definition (markdown)
│
├── agents/
│   └── acme-reviewer.md                    # Sub-agent definition (markdown)
│
└── collections/
    └── acme-frontend.json                  # Marketplace collection (Automatic JSON format)
```

## What happens on install

When a user runs `automatic://install?repo=acme/ai-toolkit`:

1. Automatic clones the repo to `~/.automatic/sources/acme/ai-toolkit/`
2. Parses `automatic.json`
3. Shows a confirmation dialog listing all resources
4. On confirm:
   - `acme-react-patterns` skill dir copied to `~/.agents/skills/acme-react-patterns/`
   - `acme-deploy` skill dir copied to `~/.agents/skills/acme-deploy/`
   - `acme-db.json` copied to `~/.automatic/mcp_servers/acme-db.json`
   - `acme-code-style.md` wrapped and written to `~/.automatic/rules/acme-code-style.json`
   - `acme-claude-hints.md` wrapped and written to `~/.automatic/rules/acme-claude-hints.json`
   - `acme-fullstack-project.json` copied to `~/.automatic/project_templates/acme-fullstack-project.json`
   - `acme-deploy.md` copied to `~/.automatic/commands/acme-deploy.md`
   - `acme-reviewer.md` copied to `~/.automatic/agents/acme-reviewer.md`
   - `acme-frontend.json` collection appended to `~/.automatic/marketplace/collections.json`
5. Source registered in `~/.automatic/sources.json`
6. Provenance recorded in `~/.automatic/source-provenance.json`

## Agent overrides in action

From the manifest's `agent_overrides`:

- **All agents** get: `acme-react-patterns`, `acme-deploy` skills + `acme-db` MCP server + `acme-code-style` rule + `acme-deploy` command + `acme-reviewer` agent
- **Claude Code** additionally gets: `acme-claude-hints` rule (via `include_rules`)
- **Cursor** does NOT get: `acme-db` MCP server (via `exclude_mcp_servers`)

## Minimal example

A source with only skills needs nothing more than a `skill.json`:

```
.
└── skill.json
└── skills/
    └── my-skill/
        └── SKILL.md
```

No `automatic.json` is required. Automatic falls back to `skill.json` automatically.
