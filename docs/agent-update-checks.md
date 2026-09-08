# Agent Update Checks

State file for the `weekly-automatic-check` scheduled task. One row per agent,
recording the date its vendor sources were last reviewed and a one-line log of
what that review found.

Prior full audit: [upstream-audit-2026-07-30.md](./agents/upstream-audit-2026-07-30.md),
remediated through 2026-08-11 (see
[agent-gap-remediation-plan.md](./agents/agent-gap-remediation-plan.md)).
Z Code and Kimi Code were added 2026-08-17 and were not part of that audit.
Global MCP discovery paths were corrected 2026-08-26.

| Agent | Last checked | Findings |
|---|---|---|
| Claude Code | 2026-09-06 | No new hook events. Docs list 33, `CLAUDE_CODE_EVENTS` still has 30 — the three missing are exactly those in VEL-158. 2.1.259 adds `managedMcpServers` (managed-settings scope, not Automatic's). 2.1.257 fixes `.claude/` settings created after startup not being read until restart, which removes a first-sync caveat. |
| Codex CLI | 2026-09-06 | 0.153.0 (2026-09-03): plugin CLI gains remote-marketplace install/remove — see VEL-159. MCP tool approvals scoped per app account; no config-shape change. `.codex/config.toml` project scope reconfirmed (trusted projects only). VEL-154 stays unconfirmed. **Config docs page not located; `learn.chatgpt.com/docs/codex/config` and `/configuration` both 404.** |
| Cursor | 2026-09-06 | No relevant changes. Latest entry 2026-09-02 (self-hosted machines). `.cursor/mcp.json` / `mcpServers` reconfirmed. |
| Kiro | 2026-09-06 | "Powers" confirmed to be Agent Plugins (`plugin.json`, `skills/`, `mcp.json`, `dev.kiro/`) — VEL-159. IDE 1.0.437 / CLI 2.21.0 (2026-09-01) sync steering, agents, Skills, Powers and Hooks from Kiro Web without writing local `.kiro` files: a second source of truth, relevant to drift wording. `.kiro/skills/` reconfirmed. |
| Gemini CLI | 2026-09-06 | No relevant changes through v0.59.0-preview.0. Security hardening only (workspace-trust fail-closed, `mcpServers` filtered in restricted mode, OAuth SSRF). No config-shape change. |
| GitHub Copilot | 2026-09-06 | No changelog entries on Automatic's surface. But VS Code documents Agent Plugin discovery at `~/.copilot/installed-plugins/` and `chat.pluginLocations`, namespace `com.github.copilot/` — VEL-159. `.vscode/mcp.json` `servers` key and required `type` reconfirmed; VEL-153 stands. |
| Cline | 2026-09-06 | **Gap.** Desktop v0.0.23 (2026-09-03) discovers Agent Plugins from `~/.agents/plugins`, validates `plugin.json`, loads their skills and starts their MCP servers. Workspace `.agents/plugins` intentionally ignored. VEL-159. v4.1.17 adds a 10s remote-MCP connect timeout (behavioural, no action). |
| Kilo Code | 2026-09-06 | No relevant changes. All four project paths (`kilo.json`, `kilo.jsonc`, `.kilo/kilo.json[c]`) still read and deep-merged, global `~/.config/kilo/kilo.json[c]`, `mcp` key with `type: local/remote` — matches `kilo_code.rs`. Docs URL moved to `kilo.ai/docs/automate/mcp/using-in-kilo-code`. |
| Junie | 2026-09-06 | No relevant changes. `mcpServers` key reconfirmed; the plugin MCP settings page still does not restate file paths, so the 2026-07-30 paths stand. |
| Kimi Code | 2026-09-06 | No relevant changes. `.kimi-code/mcp.json`, `~/.kimi-code/mcp.json`, `mcpServers`, and stdio/HTTP/`transport: sse` inference all reconfirmed. |
| Warp | 2026-09-06 | No relevant changes. Changelog through 2026.09.02 is MCP credential handling only. `~/.warp/.mcp.json`, `.warp/.mcp.json` and the `.agents/.mcp.json` third-party path all reconfirmed. Gap unchanged, VEL-151. |
| Goose | 2026-09-06 | No relevant changes to Automatic's surface. v1.49.0 (2026-09-03) adds a `PreToolUseResult` hook event; Automatic sets `hooks: false` for Goose, so this is a pre-existing gap, not a new one. No config-shape change. |
| OpenCode | 2026-09-06 | No relevant changes through v1.18.29. Provider and auth fixes only. Prior optional-field gaps carry over. |
| Droid | 2026-09-06 | No relevant changes. CLI v0.208.0–v0.209.0 (2026-08-29 to 09-01): MCP resilience and a Windows plugin-hook fix. `.factory/mcp.json` / `mcpServers` and the field list (`type` optional for stdio, required for remote) reconfirmed. |
| Antigravity | 2026-09-06 | No change. Workspace `.agents/mcp_config.json` and `serverUrl` for remote entries reconfirmed from the MCP docs page. Gap unchanged, VEL-150. **Changelog page 404s.** |
| Z Code | 2026-09-06 | **Breaking, unchanged.** Load-path table still gives `mcp.servers` for `.zcode/config.json` and `~/.zcode/cli/config.json`. VEL-157, with a comment added: the `.agents/mcp.json` compatibility path is not a safe substitute because Z Code skips it entirely once the `.zcode` config has any server. |
| Pi | 2026-09-06 | No relevant changes. `pi-mcp-adapter` 2.32.1 (2026-09-01) is `/mcp` panel and setup-flow work. `.pi/mcp.json` as highest-precedence Pi layer and `~/.pi/agent/mcp.json` global both reconfirmed against the adapter README. |
| Zed | 2026-09-06 | No relevant changes through v1.18.1. `context_servers` unchanged. |
## Tickets raised, 2026-08-28

| Ticket | Agent | Classification |
|---|---|---|
| VEL-150 | Antigravity | Gap — write workspace MCP config to `.agents/mcp_config.json`; `mcp_note` is now wrong |
| VEL-151 | Warp | Gap — write project MCP config to `.warp/.mcp.json` |
| VEL-152 | Cline | Gap — project-scoped `.cline/agents/` and `.cline/hooks/` |
| VEL-153 | GitHub Copilot | Behavioural — write explicit `"type": "stdio"` into `.vscode/mcp.json` |
| VEL-154 | Codex CLI | Gap, unconfirmed — `Interrupt` hook event |
| VEL-155 | Kiro, Copilot, Z Code, Droid | Gap — scope agent plugin bundle formats |
| VEL-156 | Claude Code | Gap — `experimental.cacheTtl`, `headersHelper`, claude.ai connector entries |

## Tickets raised, 2026-08-30

| Ticket | Agent | Classification |
|---|---|---|
| VEL-157 | Z Code | Breaking — project and global MCP configs are written under `mcpServers`; Z Code reads `mcp.servers`. Global write target `~/.zcode/config.json` is not a documented load path |
| VEL-158 | Claude Code | Gap — hook event picker missing `DirectoryAdded`, `PreModelSwitch`, `PostModelSwitch` |

## Tickets raised, 2026-09-06

| Ticket | Agent | Classification |
|---|---|---|
| VEL-159 | Cline, Kiro, Copilot, Codex CLI | Gap — Agent Plugins 1.0.0, a vendor-neutral bundle of `plugin.json` + `skills/` + `mcp.json`. Automatic emits and consumes nothing in this format |

Comments added to VEL-155 (superseded by VEL-159) and VEL-157 (why `.agents/mcp.json` is not a safe substitute for the nested `mcp.servers` key).

## Spec corrections needed, 2026-09-06

- `automatic-meta/general/agents/` has no `agent-plugins.md`. The Agent Plugins 1.0.0 spec now spans Cline, Kiro, Copilot/VS Code and Codex CLI, so it needs a shared page rather than four per-agent notes.
- `automatic-meta/general/agents/cline.md` does not mention plugins at all. Add `~/.agents/plugins` discovery, `plugin.json` validation, and that workspace `.agents/plugins` is ignored.
- `automatic-meta/general/agents/kiro.md` does not mention Powers or Kiro Web cloud sync of steering, agents, skills and hooks.
- The Kilo Code MCP docs URL is now `kilo.ai/docs/automate/mcp/using-in-kilo-code`. Update the source list in the task skill file.

## Spec corrections needed, 2026-08-30

- `automatic-meta/general/agents/zcode.md` lines 23, 26, 30, 43, 45 — the `mcpServers` project key is wrong, and the "bare top-level server map" claim comes from a docs sentence about the settings panel's Full configuration paste box, not about file parsing.
- The Codex CLI docs base URL moved from `developers.openai.com/codex` to `learn.chatgpt.com/docs` (308 redirect). Update the source list in this task's skill file and in `codex-cli.md`.

## Sources unreachable, 2026-09-06

- `antigravity.google/docs/changelog` returned 404. The MCP docs page was reachable, so the config shape was still verified.
- `kiro.dev/docs/cli/agents/` returned 404. Kiro's on-disk custom-agent and hook paths were not re-verified this run; the 2026-08-28 paths carry forward.
- The Codex CLI config reference was not located. `learn.chatgpt.com/docs/codex/config` and `/configuration` both 404. Codex was checked from its GitHub release notes, which are a primary source.
- Junie's plugin MCP settings page still does not restate file paths, only the `mcpServers` key.

## Sources unreachable, 2026-08-30

None. `docs.factory.ai` resolved normally that run, so Droid's MCP page was re-verified. Its changelog lives at `docs.factory.ai/changelog/release-notes`, not `/changelog`.

## Sources unreachable, 2026-08-28

- `docs.factory.ai` failed DNS resolution repeatedly. The MCP configuration page was retrieved earlier in the run, but the changelog and hooks pages were not. Droid's hook event list and custom-droid paths were not re-verified on 2026-08-28.
