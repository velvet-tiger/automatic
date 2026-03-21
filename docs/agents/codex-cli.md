# Codex CLI

**ID:** `codex`  
**Docs:** https://github.com/openai/codex

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |
| Sub-agents | ✓ |

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

## Sub-agents

Project: `.codex/agents/<name>.toml`  
Global: `~/.codex/agents/<name>.toml`

Format: TOML configuration file.

```toml
name = "reviewer"
description = "PR reviewer focused on correctness and security."
model = "gpt-5.4"
model_reasoning_effort = "high"
sandbox_mode = "read-only"

developer_instructions = """
Review code like an owner.
Check for correctness, security risks, and missing test coverage.
"""
```

**Note:** Automatic stores agents in canonical Markdown+YAML format (`~/.automatic/agents/*.md`) and converts to TOML when syncing to Codex projects.

## Detection

- `.codex/config.toml` exists

## Notes

- MCP format is TOML, not JSON — unique among supported agents
- Global MCP config: `~/.codex/config.toml` (same TOML format)
