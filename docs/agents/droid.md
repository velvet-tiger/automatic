# Droid

**ID:** `droid`  
**Docs:** https://docs.factory.ai  
**Vendor:** Factory.ai

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |

## Project instructions

File: `AGENTS.md` at the project root (and any parent directory up to the repo root).  
Personal global override: `~/.factory/AGENTS.md`

## MCP config

File: `.factory/mcp.json` at the project root.  
Format: `mcpServers` key. **Every entry includes an explicit `"type"` field** (`"stdio"` or `"http"`) — unlike most other agents which omit `type` for stdio.

```json
{
  "mcpServers": {
    "my-server": {
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "@example/server"]
    },
    "remote": {
      "type": "http",
      "url": "https://example.com/mcp"
    }
  }
}
```

Global: `~/.factory/mcp.json` (same format).

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.factory/mcp.json` exists

## Notes

- Explicit `"type"` on every entry is required by Droid's schema (confirmed via Factory docs)
- Global user-level servers discovered from `~/.factory/mcp.json`
