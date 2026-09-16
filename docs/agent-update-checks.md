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
| Claude Code | 2026-09-13 | No relevant changes. 2.1.265–2.1.270 (Sep 8–12) are bug fixes: `http`+legacy-SSE MCP connect fix (2.1.266), CLAUDE.md/memory attribution rules now win over the attribution reminder (2.1.269), `effort:` frontmatter fix on commands/skills/subagents (2.1.267), `claude plugin eval` and `--json` plugin flags (2.1.268/269). No instruction-file, skills-schema, `.mcp.json`, or hook-event change. |
| Codex CLI | 2026-09-13 | No dated, documented config-relevant change. In-window releases are `0.155.0-alpha.2`–`0.155.0-alpha.3.10` (Sep 10–11, no note bodies) plus Python SDK 0.154.0 (`max`/`ultra` effort, `ExternalMessage`). Alphas ship without notes, so a silent config change cannot be fully ruled out. VEL-154 stays unconfirmed. |
| Cursor | 2026-09-13 | No relevant changes. "Cursor Projects" (Sep 10) is a cloud multi-agent capability with no config-file impact. `.cursor/mcp.json` / `mcpServers` unchanged. |
| Kiro | 2026-09-13 | No relevant changes. IDE unchanged (1.0.437). CLI 2.21.2 (Sep 8), 2.21.3 (Sep 10), 2.21.4 (Sep 11) are patches; 2.21.2/2.21.3 have no published notes. 2.21.4 is session-search / harness-flag work. `.kiro/skills/` unchanged. |
| Gemini CLI | 2026-09-13 | No relevant changes. v0.59.0 stable and v0.60.0-preview.0 (Sep 8) are MCP OAuth security hardening (SSRF prevention, RFC 9207 issuer ID) and restricted-mode `mcpServers` filtering. No config-shape or instruction-file change. |
| GitHub Copilot | 2026-09-13 | **Gap.** VS Code 1.137 (Sep 9) adds optional `sandbox` / `sandboxEnabled` fields on `.vscode/mcp.json` server entries (macOS/Linux) — VEL-163. `servers` key and required `type` unchanged. The Agent Host user path `~/.copilot/mcp-config.json` is already discovered by Automatic (`github_copilot.rs:351`), so it is not a new gap. |
| Cline | 2026-09-13 | No relevant changes. Desktop v0.0.24–v0.0.26 (Sep 9–11) are Customize-view / sidebar / provider-picker UI work. No config path, key, transport, or env change. |
| Kilo Code | 2026-09-13 | No relevant changes to write targets. v7.6.0 (Sep 10) adds an opt-in one-time import of Claude Code instructions, simple skills, and disabled MCP defs *into* Kilo. Import direction only; imported MCP arrives disabled. AGENTS.md / `.kilo/kilo.json` / `.agents/skills/` unchanged. |
| Junie | 2026-09-13 | No relevant changes. 26.9.7 (Sep 7) adds response streaming and the built-in `/branch` command. Docs pages restamped 11 Sep but describe existing behaviour. `.junie/mcp/mcp.json` / `mcpServers` and skills paths intact. |
| Kimi Code | 2026-09-13 | No relevant changes. Latest changelog entry is 0.41.0 (Sep 4), before the window. `.kimi-code/mcp.json`, `mcpServers`, and transport inference all stand. |
| Warp | 2026-09-13 | No relevant changes. Changelog through 2026.09.09 (Grok support, remappable keybindings, shell completions). Nothing touching AGENTS.md, MCP paths/keys, skills, or credentials. Gap unchanged, VEL-151. |
| Goose | 2026-09-13 | No config-shape change. v1.50.0 (Sep 8) adds "Prefer latest MCP version" (protocol negotiation) and "Enforce subagent platform guards" (runtime guard) — neither changes the global YAML schema or subagent file format. `hooks: false` gap unchanged. |
| OpenCode | 2026-09-13 | No relevant changes. v1.18.30 (Sep 9) is model/provider only (GPT-6 Astra prompts, Bedrock model IDs, SDK bumps). `opencode.json` / `mcp` key unchanged. Prior optional-field gaps carry over. |
| Droid | 2026-09-13 | No relevant changes. No release in the window; latest is CLI v0.209.0 (Sep 1). `.factory/mcp.json` / `mcpServers` and the field list unchanged. |
| Antigravity | 2026-09-13 | **Gap (surface grown).** `google-antigravity/antigravity-cli` 1.1.28–1.2.2 (Sep 9–12): custom-agent `excludeDefaultComponents` frontmatter (1.2.1), plugin MCP auto-namespacing `<plugin>_<server>` in `mcp_config.json` (1.2.2), URL-fetch permission default flipped (1.1.28). Custom agents (`agents: false`), hooks, and `rules.json` are unsupported — VEL-164. The `mcp_config.json` / `serverUrl` writer is current. **`antigravity.google/docs/changelog` still 404s; used the GitHub releases API.** |
| Z Code | 2026-09-13 | No dated primary source — the vendor publishes no changelog or release notes. The MCP services doc is unchanged; the VEL-157 breaking state (`mcp.servers` vs written `mcpServers`) stands. |
| Pi | 2026-09-13 | No relevant changes. No `pi-mcp-adapter` release in the window; latest is 2.32.1 (Sep 1). `.pi/mcp.json` precedence and `~/.pi/agent/mcp.json` global unchanged. |
| Zed | 2026-09-13 | No relevant changes. v1.19.2 stable (Sep 9) adds an `ask_user` agent tool (runtime capability). No `context_servers` / `.zed/settings.json`, instruction-file, skills, or subagent change. |
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

## Tickets raised, 2026-09-13

| Ticket | Agent | Classification |
|---|---|---|
| VEL-163 | GitHub Copilot | Gap — evaluate writing optional `sandbox` / `sandboxEnabled` fields into `.vscode/mcp.json` (VS Code 1.137). Opt-in security; no regression if unwritten |
| VEL-164 | Antigravity | Gap — re-verify and scope the `antigravity-cli` config surface (custom agents with frontmatter, `PostInvocation` hooks, `rules.json`); `agents: false` today |

## Spec corrections needed, 2026-09-13

- `automatic-meta/general/agents/antigravity.md` should be re-verified against the now-active `google-antigravity/antigravity-cli` repo. The CLI documents custom agents (Markdown frontmatter incl. `mainAgent`, `subagent`, `inheritMcp`, `commandExecutionPolicy`, `excludeDefaultComponents`), `PostInvocation` hooks, `rules.json`, and skills `disable-slash-command` / `metadata.icon`. Tracked in VEL-164.

## Sources unreachable, 2026-09-13

- `antigravity.google/docs/changelog` still returned 404. The Antigravity CLI was checked via the `google-antigravity/antigravity-cli` GitHub releases API (dated, with bodies) and the IDE changelog at `antigravity.google/changelog/`, both reachable.
- Z Code (`zcode.z.ai`) publishes no changelog or release-notes page, so no in-window change could be confirmed or denied from a dated primary source. The MCP services doc was reachable and unchanged.
- Codex CLI in-window releases are `0.155.0-alpha.*` prereleases that ship without release-note bodies; a silent config change cannot be fully ruled out from the release feed alone.
- Kiro CLI 2.21.2 and 2.21.3 have no published descriptions; inferred to be bug-fix patches from the absence of notes.

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
