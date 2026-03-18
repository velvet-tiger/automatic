# GitHub Copilot

**ID:** `copilot`  
**Docs:** https://docs.github.com/en/copilot  
**Vendor:** GitHub / Microsoft

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |

## Project instructions

File: `.github/copilot-instructions.md` at the project root.  
Global: Copilot settings in VS Code / JetBrains.

## MCP config

File: `.vscode/mcp.json` at the project root.  
Format: `servers` key (not `mcpServers` — VS Code convention), stdio entries omit `"type"`.

```json
{
  "servers": {
    "my-server": {
      "command": "npx",
      "args": ["-y", "@example/server"]
    }
  }
}
```

Automatic merges into this file to preserve other VS Code settings.

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.github/copilot-instructions.md` exists, or
- `.vscode/mcp.json` exists

## Notes

- Uses `"servers"` key, not `"mcpServers"` — unique among supported agents
- Shared with VS Code's own MCP configuration
