# Cline

**ID:** `cline`  
**Docs:** https://docs.cline.bot

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | Global discovery only |
| Skills | ✓ |

## Project instructions

Directory: `.clinerules/` at the project root.  
Automatic-managed file: `.clinerules/automatic.md`  
Global rules: `~/Documents/Cline/Rules/`

Cline treats `.clinerules/` as a workspace rules directory and loads all `.md` and `.txt` files inside it.

## MCP config

File: `~/.cline/data/settings/cline_mcp_settings.json` by default.  
Override: `$CLINE_DIR/data/settings/cline_mcp_settings.json`  
Format: `mcpServers` key, stdio entries omit `"type"`.

This is global CLI state, not a project file in the repo. Automatic can discover it, but does not sync per-project MCP configuration for Cline.

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
Also supported in-project: `.clinerules/skills/`, `.claude/skills/`  
Global: `~/.cline/skills/<name>/SKILL.md`

## Hooks and Workflows

Project hooks: `.clinerules/hooks/`  
Project workflows: `.clinerules/workflows/`  
Global hooks: `~/Documents/Cline/Hooks/`  
Global workflows: `~/Documents/Cline/Workflows/`

## Detection

- `.clinerules/` exists, or
- `.cline/skills/` exists

## Notes

- Current Cline CLI docs treat `.clinerules/` as the primary workspace rules directory
- Automatic writes a single managed rule file inside that directory instead of targeting the directory path itself
- Cline also supports hooks and workflows under `.clinerules/`, but Automatic does not currently manage those resources through the Cline agent adapter
