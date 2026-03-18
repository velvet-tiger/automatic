# Antigravity

**ID:** `antigravity`  
**Docs:** https://antigravity.google/docs (requires auth)  
**Vendor:** Google

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✗ |
| Skills | ✓ |

## Project instructions

File: `GEMINI.md` at the project root.  
Confirmed via community testing — Antigravity does **not** read `AGENTS.md` despite the open standard.  
Global: `~/.gemini/GEMINI.md` (shared with Gemini CLI).

Source: https://www.reddit.com/r/google_antigravity/comments/1pgpwlk/

## MCP config

**Not supported by Automatic.** Antigravity manages MCP servers globally via its own UI:  
Agent session → "…" → MCP Servers → Manage MCP Servers → View raw config.

The config file is named `mcp_config.json`. Format uses `mcpServers` key with standard stdio entries (no explicit `type` field):

```json
{
  "mcpServers": {
    "sequential-thinking": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-sequential-thinking"]
    }
  }
}
```

The exact filesystem path of `mcp_config.json` is not yet publicly documented.  
**TODO:** Once confirmed, implement `write_mcp_config` and `discover_global_mcp_servers`.

## Rules system

In addition to `GEMINI.md`, Antigravity has a project-level rules system:  
`.agents/rules/<name>.md` (workspace) — individual Markdown files, each with an activation mode:
- **Always On** — applied to every session
- **Manual** — activated via `@rule-name` mention
- **Model Decision** — model decides based on a natural language description
- **Glob** — applied when edited files match a pattern

Legacy path `.agent/rules/` is still supported.

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.gemini/antigravity/skills/<name>/SKILL.md` (not synced by Automatic)

Legacy path `.agent/skills/` is still supported.

Skills use standard SKILL.md format with YAML frontmatter (`name`, `description`).

## Detection

- `.antigravity/` directory exists (created by the Antigravity app itself)
