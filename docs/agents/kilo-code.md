# Kilo Code

**ID:** `kilo`  
**Docs:** https://kilocode.ai/docs  
**Note:** Open-source fork of Cline / Roo Code, available for VS Code and JetBrains.

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |

## Project instructions

File: `AGENTS.md` at the project root (open standard).

## MCP config

File: `.kilocode/mcp.json` at the project root.  
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

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.kilocode/` directory exists
