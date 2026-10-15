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
| Claude Code | 2026-10-11 | No breaking change. 2.1.293–2.1.296 (Oct 7–9): subagent frontmatter `autoCompactWindow` (gap, minor), hook `onFailure: "block"`, MCP tool-description limits raised, claude.ai connectors negotiate MCP 2026-07-28. `.mcp.json`/`CLAUDE.md`/skills unchanged. 2.1.293 notes truncated. |
| Codex CLI | 2026-10-11 | Inconclusive. 0.162.1 (Oct 9) has no release notes in the API listing. No change confirmed. |
| Cursor | 2026-10-11 | No relevant change. Only Oct 6 entry: iOS remote control for local agents. |
| Kiro | 2026-10-11 | **Behavioral — VEL-177 (new).** CLI 2.28.0 (Oct 5): switch Classic to 3.0 upgrades agent configs; category-based hook matching. IDE 1.2.56 (Oct 8): hook matching. VEL-175 remains. Summarised fetch only; unverified. |
| Gemini CLI | 2026-10-11 | No relevant change. v0.63.0 stable (Oct 6), v0.64.0-preview.1 (Oct 9), v0.65.0 nightlies: read-only workspace settings in untrusted folders (behavioral, nightly only), a2a-server V1→V2 settings migration. Watch. |
| GitHub Copilot | 2026-10-11 | No breaking change. VS Code MCP docs now document portable `.mcp.json`/`~/.copilot/mcp-config.json` (`mcpServers`) alongside `servers`; comment added to VEL-163. Oct 9 weekly changelog body not read. |
| Cline | 2026-10-11 | No relevant change. CLI v3.0.69/.70, SDK v0.0.91/.92, Desktop v0.0.44/.45 (Oct 7–8): stdio startup timeout 10s, EPIPE fixes, rules loading exports. |
| Kilo Code | 2026-10-11 | No relevant change found. v7.8.8 (Oct 7); release notes not in listing. |
| Junie | 2026-10-11 | No relevant change. MCP page updated 8 Oct 2026; still `mcpServers`, no paths on page. |
| Kimi Code | 2026-10-11 | No relevant change. MCP docs match `.kimi-code/mcp.json`/`mcpServers`; new experimental `deferred`; project servers in untrusted folders shown in trust prompt. Changelog after 2.1.0 not reachable. |
| Warp | 2026-10-11 | Inconclusive. Changelog last updated Oct 9; fetch returned no entry detail. |
| Goose | 2026-10-11 | No config-shape change. v1.54.0 (Oct 8): extension leases, MCP protocol version in initialize. Extensions YAML unchanged. |
| OpenCode | 2026-10-11 | **Behavioral — VEL-176 (new).** v1.19.0 (Oct 11): `opencode upgrade --major` to OpenCode 2 (beta). Docs say V1 config is read in memory; breaking only for server and plugin APIs. v1.18.35 (Oct 6) notes truncated. |
| Droid | 2026-10-11 | No relevant change. CLI v0.234.0 (Oct 6), v0.236.0 (Oct 8): hook can block submitted message, plugin install keeps MCP and droid files. `.factory/mcp.json` unchanged. |
| Antigravity | 2026-10-11 | **Gap — VEL-164 comment.** CLI 1.2.17–1.3.3 (Oct 5–10): plugin system (`.antigravity-plugin/`, `.gemini-plugin/`, `agents.json`, `skills.json`), custom-agent relative paths, BOM-tolerant `mcp_config.json`. No breaking change. |
| Z Code | 2026-10-11 | No relevant change. v3.14.5 (Oct 9): reset display fix only. |
| Pi | 2026-10-11 | No relevant change. `pi-mcp-adapter` v5.1.0 (Oct 6, `openUi` entry key), v5.2.0 (Oct 9, redirects not followed, trust doctor). Paths unchanged; keep `.pi/mcp.json`. |
| Zed | 2026-10-11 | No relevant change. v1.23.2 (Oct 7): MCP optional-field fix for OpenAI tool calls. `context_servers` unchanged. |
## Run 2026-10-11 (window 2026-10-04 to 2026-10-11)

Tickets: **VEL-176** (new, OpenCode 2, Low), **VEL-177** (new, Kiro 3.0, Low). Comments: **VEL-164** (Antigravity plugins), **VEL-163** (Copilot portable MCP format).

## Sources unreachable or limited, 2026-10-11

- Research used WebFetch summaries over GitHub releases APIs. Pages were truncated at 100k characters for Claude Code, Codex, Kilo, OpenCode, Zed, Goose and Droid.
- Codex 0.162.1, Kilo v7.8.8 and OpenCode v1.18.35 had no readable notes.
- Warp changelog and Copilot Oct 9 roundup showed titles only.

## Run 2026-10-04 (window 2026-09-27 to 2026-10-04)

Tickets: **VEL-175** (new, Kiro gap/behavioral, Low). Comments: **VEL-171** (Pi: adapter v5.0.0 reverses the rename, keep `.pi/mcp.json`), **VEL-172** (Kilo: `.agents/skills/` verified read, suggest close).

## Spec corrections needed, 2026-10-04

- `automatic-meta/general/agents/pi.md` should record the adapter v3.0.0 rename and the v5.0.0 reversal (both `.pi/mcp.json` and `mcp-adapter.json` read; project trust/approval gate).
- `automatic-meta/general/agents/kilo-code.md` can note the documented project skills order (`.kilo/skills/`, `.agents/skills/`, `.claude/skills/`).
- The Droid docs host is now `docs.factory.com`. Update the source list in the task skill file.

## Sources unreachable or limited, 2026-10-04

- Research used WebFetch summaries, so detail is partial. Docs pages (code.claude.com, learn.chatgpt.com, cursor.com/docs, kiro.dev/docs, goose-docs.ai, docs.cline.bot, opencode.ai/docs, Z Code and Droid MCP pages) were mostly not re-fetched.
- Codex release bodies did not load (alpha builds only).
- Junie changelog URL 404; `antigravity.google/changelog/` returned a redirect notice.
- Warp Oct 1–2 entries may be unindexed.

## Tickets raised, 2026-09-27

| Ticket | Agent | Classification |
|---|---|---|
| VEL-171 | Pi | Breaking — `pi-mcp-adapter` v3.0.0 renames the adapter's MCP config `.pi/mcp.json` → `.pi/mcp-adapter.json` (project) and `~/.pi/agent/mcp.json` → `~/.pi/agent/mcp-adapter.json` (global); Automatic's writes to `.pi/mcp.json` stop being read after upgrade. Plus behavioral first-connection approval. |
| VEL-172 | Kilo Code | Gap (needs verification) — marketplace companion-skills install to `.kilo/skills/`; docs suggest project skills are read only from `.kilo/skills/`, while Automatic writes `.agents/skills/`. Verify Kilo's project read paths before changing `skill_dirs`. |

Comment added to VEL-164 (Antigravity): CLI v1.2.11 (Sep 25) confirms project custom agents at `.agents/agents/<name>.md`; v1.2.9 `@<subagent>` messaging; v1.2.10 manifest dir-scan depth aligned with `.agents/skills/`. No new ticket — path pinned for when VEL-164 is picked up.

## Spec corrections needed, 2026-09-27

- `automatic-meta/general/agents/cline.md` (≈lines 50-54) still places project hooks at `.clinerules/hooks/` and says Automatic "does not currently manage those resources", but VEL-152 shipped and `cline.rs::sync_cline_hooks` writes `.cline/hooks/<Event>.<ext>` with the hooks capability enabled. Reconcile the spec to the shipped code (`.cline/hooks/`, hooks managed) and confirm Cline's actual project hook read path while doing so.
- `automatic-meta/general/agents/warp.md` still marks MCP as "✗ / not synced" and calls project MCP write "future work", but `warp.rs` already implements a `.warp/.mcp.json` project writer + discovery (VEL-151, Done). Update the spec to match.
- `automatic-meta/general/agents/pi.md` should record the `pi-mcp-adapter` v3.0.0 file rename (`.pi/mcp-adapter.json`) once VEL-171 is resolved.
- `automatic-meta/general/agents/antigravity.md` should record the confirmed project custom-agent path `.agents/agents/<name>.md` (tracked via the VEL-164 comment).

## Sources unreachable, 2026-09-27

- `antigravity.google/docs/changelog` still returned 404 (as in prior runs). The Antigravity CLI was checked via the `google-antigravity/antigravity-cli` GitHub releases API (dated, with bodies) and the IDE changelog at `antigravity.google/changelog/`.
- Codex CLI in-window prereleases (`0.158.*` / `0.159.0-alpha.*`) and the 0.157.1 point release carried thin or empty note bodies; substance was taken from the 0.157.0 release body. A silent prerelease-only config change cannot be fully ruled out from the feed alone.
- Warp publishes its changelog weekly; a Sep 23/24 entry may not yet be published or indexed. Latest confirmed dated entry remains 2026.09.16.
- Kilo `v7.8.0` third-party aggregator page (ppcbasic.com) 404'd; specifics were taken from the GitHub releases listing plus Kilo docs. One aggregator's "mcp.servers" phrasing for Kilo was not corroborated by primary docs (plain `mcp` key) and was treated as imprecision.
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
