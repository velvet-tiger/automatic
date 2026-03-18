# Cline

**ID:** `cline`  
**Docs:** https://docs.cline.bot

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |

## Project instructions

File: `.clinerules` at the project root (file or directory).  
Global: `~/Documents/Cline/Rules/`

## MCP config

File: `.cline/mcp.json` at the project root.  
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

## Skills

Project: `.cline/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.cline/mcp.json` exists, or
- `.clinerules` exists

## Notes

- `.clinerules` can be a single file or a directory containing multiple rule files
