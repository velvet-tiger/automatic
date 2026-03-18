# Cursor

**ID:** `cursor`  
**Docs:** https://docs.cursor.com

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |

## Project instructions

File: `.cursorrules` at the project root (legacy, still supported).  
New format: individual rule files under `.cursor/rules/` (MDC format with YAML frontmatter).  
Global: Cursor settings UI.

## MCP config

File: `.cursor/mcp.json` at the project root.  
Format: `mcpServers` key, stdio entries omit `"type"`, http entries include `"type": "http"`.

```json
{
  "mcpServers": {
    "my-server": {
      "command": "npx",
      "args": ["-y", "@example/server"]
    }
  }
}
```

Global: `~/.cursor/mcp.json` (same format).

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.cursor/mcp.json` exists, or
- `.cursorrules` exists, or
- `.cursor/rules/` directory exists
