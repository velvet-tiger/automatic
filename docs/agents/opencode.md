# OpenCode

**ID:** `opencode`  
**Docs:** https://opencode.ai/docs  
**Vendor:** SST

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |

## Project instructions

File: `AGENTS.md` at the project root (open standard).

## MCP config

File: `opencode.json` at the project root (also detects `.opencode.json`).  
Format: `mcp` key with named server objects. Each server has a `type` field: `"local"` (stdio) or `"remote"` (http/sse).

```json
{
  "mcp": {
    "my-server": {
      "type": "local",
      "command": "npx",
      "args": ["-y", "@example/server"]
    },
    "remote-service": {
      "type": "remote",
      "url": "https://example.com/mcp"
    }
  }
}
```

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `opencode.json` exists, or
- `.opencode.json` exists, or
- `.agents/skills/` directory exists

## Notes

- Uses `"mcp"` key (not `"mcpServers"`) and `"type": "local"` / `"type": "remote"` (not `"stdio"` / `"http"`)
- Unique among supported agents in its MCP format
