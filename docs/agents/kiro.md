# Kiro

**ID:** `kiro`  
**Docs:** https://kiro.dev/docs  
**Vendor:** AWS

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |

## Project instructions

File: `AGENTS.md` at the project root (open standard).

## MCP config

File: `.kiro/settings/mcp.json` at the project root.  
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

Project: `.kiro/skills/<name>/SKILL.md`

## Detection

- `.kiro/` directory exists

## Notes

- Detection uses the `.kiro/` directory as the marker (not just `mcp.json`)
- Cleanup removes the entire `.kiro/` directory
