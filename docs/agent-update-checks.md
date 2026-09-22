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
| Claude Code | 2026-09-20 | **Behavioral + gap** — VEL-165. 2.1.271–2.1.278 (changelog carries no per-entry dates; treated as in-window above the 2.1.270 ceiling). AGENTS.md read as fallback when no CLAUDE.md (2.1.277); `omitClaudeMd` agent frontmatter (2.1.271); claude.ai account skill/plugin sync into terminal sessions (2.1.275); `CLAUDE_CODE_MCP_STARTUP_WAIT_MS` (2.1.274). Automatic writes CLAUDE.md and handles none of these — nothing broken. |
| Codex CLI | 2026-09-20 | No confirmed config-relevant change. `rust-v0.155.1` (Sep 18) is a TUI reasoning-summary bugfix. `0.156.0-alpha.2`–`alpha.9` (Sep 18–20) ship without note bodies, so a silent config change cannot be fully ruled out. VEL-154 stays unconfirmed. Docs base is now `learn.chatgpt.com/docs`. |
| Cursor | 2026-09-20 | No relevant changes. Newest main-changelog entry is Sep 10 ("Cursor Projects"); CLI changelog newest is Aug 26. Nothing dated in window. `.cursor/mcp.json` / `mcpServers` unchanged. |
| Kiro | 2026-09-20 | No write-target change. IDE 1.1 (Sep 14) surfaces MCP failures more clearly and makes hooks "more reliable" (both behavioral); CLI 2.22.0 (Sep 16) is fullscreen chat / V3 dashboard. `.kiro/settings/mcp.json` shape and hook format unchanged. |
| Gemini CLI | 2026-09-20 | Behavioral, no config-shape change. v0.60.0 stable (Sep 15) enforces RFC 9207 issuer ID in MCP OAuth and hardens system-config path permissions. `.gemini/settings.json` / `mcpServers` unchanged. OAuth servers not returning `iss` may now fail. |
| GitHub Copilot | 2026-09-20 | No new change. Changelog's first in-window entry is Sep 18 (code-review UX, weekly-release recap, mid-Oct model deprecation). VS Code MCP doc (updated Sep 16) still matches baseline: `servers` key, `type` required; `sandbox`/`sandboxEnabled` already VEL-163. No new field this window. |
| Cline | 2026-09-20 | Gap (already tracked). Desktop v0.0.27–v0.0.32 (Sep 13–18) UI-only. CLI v3.0.62 / SDK v0.0.83 (Sep 15) load `~/.agents/plugins` Agent Plugins (skills + MCP) without touching `cline_mcp_settings.json` — same format as VEL-159; comment added there. Global MCP writer unaffected. |
| Kilo Code | 2026-09-20 | No write-target change. v7.7.0–v7.7.5 (Sep 15–18): `@provider/model` subagent mentions, leading-slash `skills.paths` root fallback (v7.7.2), KiloClaw removed. `.kilo/kilo.json` / `mcp` key unchanged. (Baseline's `skill.paths` is actually `skills.paths`.) |
| Junie | 2026-09-20 | No relevant changes. 26.9.14 / build 3196.4 (Sep 14) adds a `/review` Apply-suggestion UI and `/goal` improvements. `.junie/AGENTS.md`, `.junie/mcp/mcp.json` / `mcpServers`, and skills paths intact. |
| Kimi Code | 2026-09-20 | No relevant changes. 0.43.0/0.43.1 (Sep 14/15) are subagent runtime fixes; 2.0.0 (Sep 17) adds `/desktop` + `kimi install-app` and terminal mermaid — runtime/UI only. `.kimi-code/mcp.json` / `mcpServers` / transport inference unchanged. (Product docs at kimi.com/code are authoritative, not the `MoonshotAI/kimi-cli` 1.x line.) |
| Warp | 2026-09-20 | No relevant changes. Changelog 2026.09.16: shell completions, `oz secret create docker-registry`, cloud-agent credential preservation, git-config fix. Nothing touching AGENTS.md, MCP paths/keys, or skills. Gap unchanged, VEL-151. |
| Goose | 2026-09-20 | No config-shape change. v1.51.0 (Sep 17): MCP preferred-version / HTTP retries, app-tool owner binding, Toolshim scoping — all runtime. Global YAML schema and (absent) subagent/hook support unchanged. |
| OpenCode | 2026-09-20 | No relevant changes. v1.18.31 (Sep 14) is ACP session restoration + Copilot adaptive-thinking only. `opencode.json` / `mcp` key unchanged. Prior optional-field gaps carry over. |
| Droid | 2026-09-20 | No write-target change. v0.220.0–v0.223.0 (Sep 16–19): org skill-disable policy (v0.222.0), hook risk-level setting + credential notices (v0.223.0), legacy custom-model migration (not MCP). `.factory/mcp.json` explicit `type:stdio`, AGENTS.md, `.factory/droids/` unchanged. |
| Antigravity | 2026-09-20 | **Behavioral/gap — reinforces VEL-164.** `antigravity-cli` 1.2.3–1.2.7 (Sep 15–19): subagent MCP inheritance / auto-guidance, `/skills reload`, `/hooks` plugin listing, legacy tool retirement, 20k-token rule budget. Custom agents / hooks still `agents:false`. Global `mcp_config.json` / `serverUrl` unchanged. **`antigravity.google/docs/changelog` still 404s; used GitHub releases API.** |
| Z Code | 2026-09-20 | **Gap — VEL-166.** v3.14.0 (Sep 19) adds dynamic workflows / `/workflow` multi-subagent orchestration (sync-mappability unverified). A vendor changelog now EXISTS (`zcode.z.ai/en/changelog`) — the prior "no changelog" note is retired. VEL-157 (`mcp.servers`) is Done and `zcode.rs` writes the nested key; the task's baseline table row (`mcpServers`) is stale. |
| Pi | 2026-09-20 | Gap/behavioral, opt-in only. `pi-mcp-adapter` v2.34.0 (Sep 14): encrypted-file OAuth store (`oauthCredentialStore`, env-gated) and bounded ancestor `.mcp.json` discovery (`ancestorConfigRoots`, default-off). `.pi/mcp.json` path/key/entry shape unchanged; no action forced. |
| Zed | 2026-09-20 | Behavioral only. v1.20.1 (Sep 16) fixes multibyte Agent Skill descriptions being wrongly rejected and a settings.json key-escaping corruption; v1.20.2 (Sep 17) follows. `context_servers` / `.zed/settings.json` shape unchanged. |
## Tickets raised, 2026-09-20

| Ticket | Agent | Classification |
|---|---|---|
| VEL-165 | Claude Code | Behavioral + gap — AGENTS.md fallback when no CLAUDE.md (2.1.277), `omitClaudeMd` agent frontmatter (2.1.271), claude.ai account skill/plugin sync (2.1.275), `CLAUDE_CODE_MCP_STARTUP_WAIT_MS` (2.1.274). Nothing Automatic writes is broken |
| VEL-166 | Z Code | Gap — v3.14.0 dynamic workflows / `/workflow` multi-subagent orchestration (sync-mappability unverified); vendor changelog now exists; skills doc references sub-agents vs baseline |

Comment added to VEL-159: Cline CLI v3.0.62 / SDK v0.0.83 (Sep 15) now also load `~/.agents/plugins` Agent Plugins, extending the tracked surface beyond Desktop v0.0.23.

## Spec corrections needed, 2026-09-20

- `automatic-meta/general/agents/claude-code.md` should record: AGENTS.md fallback precedence (read when no CLAUDE.md), the `omitClaudeMd` agent-frontmatter key, and claude.ai account skill/plugin sync. Tracked in VEL-165.
- `automatic-meta/general/agents/zcode.md` should record that a vendor changelog now exists (`zcode.z.ai/en/changelog`) and reconcile the skills doc's sub-agent references against the "no sub-agents" baseline. Tracked in VEL-166.
- The `check-agents-for-updates` task skill source list should add `https://zcode.z.ai/en/changelog` for Z Code (previously recorded as having no changelog).
- The task skill's current-baseline table is stale in two rows: Z Code project MCP is now written under nested `mcp.servers` (VEL-157 shipped), not `mcpServers`; and the Kilo Code skills setting is `skills.paths`, not `skill.paths`.

## Sources unreachable, 2026-09-20

- `antigravity.google/docs/changelog` still returned 404 (as in prior runs). The Antigravity CLI was checked via the `google-antigravity/antigravity-cli` GitHub releases API (dated, with bodies).
- Codex CLI in-window releases are `0.156.0-alpha.*` prereleases whose individual tag pages returned errors / carried no note bodies; a silent config change cannot be fully ruled out from the release feed alone.
- Z Code: one fetch attributed a "v3.12.3 MCP protocol-version" changelog entry that a verbatim re-fetch did not reproduce; treated as unconfirmed.
- Kilo `CHANGELOG.md` blob on github.com rendered without text; the equally-primary Releases page and MCP docs covered the window.

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
