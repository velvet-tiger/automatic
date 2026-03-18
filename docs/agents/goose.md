# Goose

**ID:** `goose`  
**Docs:** https://block.github.io/goose/  
**Vendor:** Block

## Capabilities

| Capability | Supported |
|---|---|
| Instructions | ✓ |
| MCP Servers | ✗ |
| Skills | ✓ |

## Project instructions

Goose checks `CONTEXT_FILE_NAMES` at session start. Default order: `["AGENTS.md", ".goosehints"]`.  
Automatic writes `AGENTS.md` (checked first).

## MCP config

**Not supported by Automatic.** Goose manages extensions globally via:  
`~/.config/goose/config.yaml` (YAML format, `extensions` key).

There is no project-level MCP config file. Extensions must be added via:
- `goose configure` CLI command, or
- Goose Desktop GUI

Automatic reads this file for global MCP discovery (import only, no write).

### Global config format

```yaml
extensions:
  my-server:
    type: stdio
    name: My Server
    cmd: npx
    args: [-y, "@example/server"]
    enabled: true
    timeout: 300

  remote-service:
    type: streamable_http
    name: Remote Service
    uri: https://example.com/mcp
    enabled: true
    timeout: 300
```

## Skills

Project: `.agents/skills/<name>/SKILL.md`  
Global: `~/.agents/skills/<name>/SKILL.md`

## Detection

- `.goosehints` file exists

## Notes

- `AGENTS.md` alone is not used for detection (too generic)
- Extension types: `stdio`, `streamable_http` (supported); `builtin`, `platform`, `sse` (deprecated/skipped on import)
- Goose fields differ from standard MCP: `cmd` not `command`, `envs` not `env`, `uri` not `url` for HTTP
