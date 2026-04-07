# Agent Reference

One file per supported agent. Each covers: capabilities, project instructions file, MCP config path and format, skills directory, sub-agents directory, and detection markers.

| Agent | ID | Instructions file | MCP config | Skills dir | Sub-agents dir |
|---|---|---|---|---|---|
| [Claude Code](./claude-code.md) | `claude` | `CLAUDE.md` | `.mcp.json` | `.claude/skills/` | `.claude/agents/` |
| [Codex CLI](./codex-cli.md) | `codex` | `AGENTS.md` | `.codex/config.toml` (TOML) | `.agents/skills/` | `.codex/agents/` |
| [Cursor](./cursor.md) | `cursor` | `.cursorrules` | `.cursor/mcp.json` | `.agents/skills/` | `.cursor/agents/` |
| [Kiro](./kiro.md) | `kiro` | `AGENTS.md` | `.kiro/settings/mcp.json` | `.kiro/skills/` | — |
| [Gemini CLI](./gemini-cli.md) | `gemini` | `GEMINI.md` | `.gemini/settings.json` | `.agents/skills/` | `.gemini/agents/` |
| [GitHub Copilot](./github-copilot.md) | `copilot` | `.github/copilot-instructions.md` | `.vscode/mcp.json` (`servers` key) | `.agents/skills/` | — |
| [Cline](./cline.md) | `cline` | `.clinerules/automatic.md` | Global CLI settings only (`~/.cline/data/settings/cline_mcp_settings.json`) | `.cline/skills/` | — |
| [Kilo Code](./kilo-code.md) | `kilo` | `AGENTS.md` | `.kilocode/mcp.json` | `.agents/skills/` | — |
| [Junie](./junie.md) | `junie` | `.junie/guidelines.md` | `.junie/mcp.json` | `.junie/skills/` | — |
| [Warp](./warp.md) | `warp` | `AGENTS.md` | — (UI only) | `.agents/skills/` | — |
| [Goose](./goose.md) | `goose` | `AGENTS.md` | — (global YAML only) | `.agents/skills/` | — |
| [OpenCode](./opencode.md) | `opencode` | `AGENTS.md` | `opencode.json` (`mcp` key) | `.agents/skills/` | `.opencode/agents/` |
| [Droid](./droid.md) | `droid` | `AGENTS.md` | `.factory/mcp.json` (explicit `type`) | `.agents/skills/` | — |
| [Antigravity](./antigravity.md) | `antigravity` | `GEMINI.md` | — (UI only, path TBD) | `.agents/skills/` | — |

## MCP format variations

Most agents use `mcpServers` + `command`/`args`. Exceptions:

- **GitHub Copilot** — uses `servers` key (VS Code format)
- **Codex CLI** — uses TOML instead of JSON
- **OpenCode** — uses `mcp` key with `type: "local"` / `type: "remote"`
- **Droid** — requires explicit `"type": "stdio"` on every entry
- **Goose** — no project file; global YAML with `cmd` (not `command`) and `uri` (not `url`)
- **Warp** — no config file at all; UI-managed
- **Antigravity** — no project file; UI-managed, path not yet documented
