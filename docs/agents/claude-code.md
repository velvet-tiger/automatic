# Claude Code

**ID:** `claude`  
**Docs:** https://docs.anthropic.com/en/docs/claude-code

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |
| Sub-agents | ✓ |

## Project instructions

File: `CLAUDE.md` at the project root.  
Global: `~/.claude/CLAUDE.md`

## MCP config

File: `.mcp.json` at the project root.  
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

Global: `~/.claude/mcp.json` (same format).

## Skills

Project: `.claude/skills/<name>/SKILL.md`  
Global: `~/.claude/skills/<name>/SKILL.md`

## Sub-agents

Project: `.claude/agents/<name>.md`  
Global: `~/.claude/agents/<name>.md`

Format: Markdown with YAML frontmatter. Fields: `name`, `description`, `tools`, `model`, `color`, `hooks`.

```markdown
---
name: code-reviewer
description: Reviews code for quality and best practices.
tools: Read, Glob, Grep, Bash
model: sonnet
---

You are a code reviewer. Analyze code and provide actionable feedback.
```

## Detection

- `.mcp.json` exists, or
- `.claude/settings.json` exists, or
- `.claude/skills/` directory exists

## Notes

- Supports writing rules as individual files under `.claude/rules/` (opt-in via agent option `claude_rules_in_dot_claude`)
- Global config also read from `~/.config/claude/`
