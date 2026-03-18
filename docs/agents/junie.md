# Junie

**ID:** `junie`  
**Docs:** https://www.jetbrains.com/help/junie/overview.html  
**Vendor:** JetBrains

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ (best-effort) |
| Skills | ✓ |

## Project instructions

File: `.junie/guidelines.md` at the project root.  
Confirmed via JetBrains documentation.

## MCP config

File: `.junie/mcp.json` at the project root.  
Format: `mcpServers` key, stdio entries omit `"type"`.

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

**Status:** Path is confirmed from JetBrains help docs. MCP support was added in 2026.1.

## Skills

Project: `.junie/skills/<name>/SKILL.md` and `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.junie/guidelines.md` exists, or
- `.junie/mcp.json` exists

## Notes

- Cleanup removes the entire `.junie/` directory
