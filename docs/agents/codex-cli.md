# Codex CLI

**ID:** `codex`  
**Docs:** https://github.com/openai/codex

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |

## Project instructions

File: `AGENTS.md` at the project root (open standard).  
Global: `~/.codex/instructions.md`

## MCP config

File: `.codex/config.toml` at the project root.  
Format: TOML, `[mcp_servers.<name>]` sections.

```toml
[mcp_servers.my-server]
command = "npx"
args = ["-y", "@example/server"]

[mcp_servers.remote]
type = "sse"
url = "https://example.com/mcp"
```

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.codex/config.toml` exists

## Notes

- MCP format is TOML, not JSON — unique among supported agents
- Global MCP config: `~/.codex/config.toml` (same TOML format)
