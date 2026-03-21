# Gemini CLI

**ID:** `gemini`  
**Docs:** https://github.com/google-gemini/gemini-cli  
**Vendor:** Google

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✓ |
| Skills | ✓ |
| Sub-agents | ✓ |

## Project instructions

File: `GEMINI.md` at the project root.  
Global: `~/.gemini/GEMINI.md`

Can be configured to read `AGENTS.md` instead via `.gemini/settings.json`:
```json
{ "context": { "fileName": "AGENTS.md" } }
```

## MCP config

File: `.gemini/settings.json` at the project root, under the `mcpServers` key.  
Automatic merges into this file rather than overwriting it (preserves other settings).

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

Global: `~/.gemini/settings.json` (same format, same merge behaviour).

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Sub-agents

Project: `.gemini/agents/<name>.md`  
Global: `~/.gemini/agents/<name>.md`

Format: Markdown with YAML frontmatter. Fields: `name`, `description`, `tools`.

```markdown
---
name: security-reviewer
description: Reviews code for security vulnerabilities.
tools:
  - read_file
  - run_shell_command
  - google_web_search
---

You are a security review specialist. Scan for common vulnerabilities.
```

Enable sub-agents in settings:
```json
{
  "experimental": {
    "enableAgents": true
  }
}
```

## Detection

- `GEMINI.md` exists, or
- `.gemini/settings.json` exists, or
- `.gemini/` directory exists
