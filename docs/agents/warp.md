# Warp

**ID:** `warp`  
**Docs:** https://docs.warp.dev  
**Vendor:** Warp

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✗ |
| Skills | ✓ |

## Project instructions

File: `AGENTS.md` at the project root (canonical as of 2025).  
Legacy: `WARP.md` (still supported for backward compatibility).  
Global: Warp Drive rules.

Automatic writes `AGENTS.md`. Both filenames are cleaned up on removal.

## MCP config

**Not supported by Automatic.** Warp manages MCP servers through its own UI:  
Settings → AI → MCP Servers, or Warp Drive → MCP Servers.

There is no project-level MCP config file.

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.warp/` directory exists, or
- `WARP.md` exists

## Notes

- `AGENTS.md` is not used as a detection marker alone (shared with other agents)
- Detection requires a Warp-specific marker (`.warp/` dir or legacy `WARP.md`)
