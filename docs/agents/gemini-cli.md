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

## Detection

- `GEMINI.md` exists, or
- `.gemini/settings.json` exists, or
- `.gemini/` directory exists
