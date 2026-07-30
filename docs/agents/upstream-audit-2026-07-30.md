# Agent Upstream Audit — 2026-07-30

Audit of every agent in `src-tauri/src/agent/` against its vendor's current
official documentation. Each finding records what upstream says, what Automatic
implements, and the verdict.

Nothing in this document has been changed in code. It is a work list.

## Method

Every claim below was read from the vendor's own documentation on the date in
the title. Third-party summaries were used only to locate the official page,
never as the source of a fact. Where no official page states a thing, the
verdict says so rather than guessing.

## Findings by priority

| # | Agent | Finding | Severity |
|---|---|---|---|
| 1 | Kilo Code | Rebranded to Kilo. Config moved to `kilo.jsonc` with an `mcp` key. `.kilocode/` is no longer read at all. Our writer produces a file Kilo ignores. | Broken |
| 2 | Junie | Junie scans only `.junie/skills/`. `sync_skills` writes to `.agents/skills/`. Synced skills are invisible to Junie. | Broken |
| 3 | Zed | No `.zed/agents` directory exists in Zed. Sub-agent files we write there are inert. | Broken |
| 4 | Gemini CLI | Gemini CLI now ships hooks (11 events, `settings.json`). We declare `hooks: false`. | Missing feature |
| 5 | GitHub Copilot | Custom agents (`.github/agents/*.agent.md`) and hooks (`.github/hooks/*.json`, 8 events) now exist. We declare both off. | Missing feature |
| 6 | Droid | Custom droids (`.factory/droids/*.md`) and hooks (`.factory/hooks.json`, 9 events) now exist. We declare both off. | Missing feature |
| 7 | Kiro | Kiro CLI custom agents (`.kiro/agents/*.json`) and agent hooks (`.kiro/hooks/`) now exist. We declare both off. | Missing feature |
| 8 | Codex CLI | Hook event list grew from 6 to 11. Five events are silently skipped at sync time. | Incomplete |
| 9 | Claude Code | `MessageDisplay` hook event missing from the UI event list. | Incomplete |
| 10 | Zed | `.rules` is now a backwards-compatibility file. `AGENTS.md` is the documented instruction file. | Drift |
| 11 | Warp | `WARP.md` takes priority over `AGENTS.md`. A legacy `WARP.md` silently shadows what we write. | Drift |
| 12 | Antigravity | Global MCP config path is now documented: `~/.gemini/config/mcp_config.json`. Resolves the open TODO in `antigravity.rs`. | Enhancement |
| 13 | Antigravity | Global skills path in our doc comment (`~/.gemini/antigravity/skills/`) is stale. It is `~/.gemini/config/skills/`. | Doc drift |
| 14 | Cline | A global `~/.cline/mcp.json` now exists for the Cline CLI. We implement no global discovery. | Enhancement |
| 15 | Codex CLI | New MCP config keys not written: `auth = "oauth"`, `env_http_headers`, `required`, `enabled_tools`/`disabled_tools`. | Enhancement |
| 16 | All | Vendor documentation URLs moved for Codex, Kilo, Junie, and Goose. Source comments point at redirects. | Doc drift |

## Per-agent detail

### Claude Code — `claude`

Verified against [code.claude.com/docs/en/settings](https://code.claude.com/docs/en/settings)
and [code.claude.com/docs/en/hooks](https://code.claude.com/docs/en/hooks).

Upstream paths, all matching our implementation:

- Instructions: `CLAUDE.md`, also `.claude/CLAUDE.md`
- MCP: `.mcp.json` (project), `~/.claude.json` (user)
- Skills: `.claude/skills/`
- Sub-agents: `.claude/agents/`
- Commands: `.claude/commands/`
- Hooks: `.claude/settings.json`

One gap. Claude Code documents 30 hook events. `CLAUDE_CODE_EVENTS` in
[src/pages/workspace/Hooks.tsx:24](../../src/pages/workspace/Hooks.tsx) lists 29.
The missing one is `MessageDisplay`, which fires while assistant message text is
displayed and takes no matcher.

Also worth noting: hooks can now be declared in skill and agent frontmatter, and
in a plugin's `hooks/hooks.json`. Automatic writes hooks only into
`.claude/settings.json`. That remains valid, but frontmatter hooks are a surface
we do not model.

### Codex CLI — `codex`

Verified against [learn.chatgpt.com/docs/extend/mcp](https://learn.chatgpt.com/docs/extend/mcp),
[learn.chatgpt.com/docs/agent-configuration/subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents)
and [learn.chatgpt.com/docs/hooks](https://learn.chatgpt.com/docs/hooks).

Correct in our implementation:

- Instructions `AGENTS.md`; MCP in `.codex/config.toml` (project scope is
  supported, trusted projects only)
- Sub-agents `.codex/agents/*.toml` with `name`, `description`,
  `developer_instructions`, plus `model`, `model_reasoning_effort`,
  `sandbox_mode`, `mcp_servers`
- Skills: `.agents/skills/` is scanned from cwd up to the repo root.
  `.codex/skills/` also exists now but is not required for our writes to work
- `env_vars` for inherited environment values, which is the right mechanism
  given TOML has no interpolation

Hook events grew. Upstream now documents 11:

```
SessionStart  SessionEnd  PreToolUse  PermissionRequest  PostToolUse
PreCompact    PostCompact UserPromptSubmit  SubagentStart  SubagentStop  Stop
```

`CODEX_SUPPORTED_EVENTS` in
[src-tauri/src/agent/codex_cli.rs:489](../../src-tauri/src/agent/codex_cli.rs)
lists 6. A hook attached to any of `SessionEnd`, `PreCompact`, `PostCompact`,
`SubagentStart` or `SubagentStop` is dropped with a warning to stderr that no
user sees. `CODEX_CLI_EVENTS` in `Hooks.tsx` has the same six and must move in
lockstep.

New MCP keys we do not write: `auth = "oauth"`, `env_http_headers`, `required`,
`enabled_tools`, `disabled_tools`, `default_tools_approval_mode`, and per-tool
`[mcp_servers.<name>.tools.<tool>]` blocks. Our canonical server model carries an
`oauth` concept already, so `auth = "oauth"` is the reachable one.

### Cursor — `cursor`

Verified against [cursor.com/docs/context/rules](https://cursor.com/docs/context/rules),
[cursor.com/docs/skills](https://cursor.com/docs/skills) and
[cursor.com/docs/agent/hooks](https://cursor.com/docs/agent/hooks).

No gaps found. This is the cleanest agent in the set.

- `AGENTS.md` is supported, including nested files in subdirectories
- `.cursor/rules/*.mdc` requires frontmatter; a plain `.md` there is ignored,
  which is what our `.mdc` gate already assumes
- Skills are read from `.agents/skills/`, `.cursor/skills/`, and the Claude and
  Codex directories for compatibility
- The 21 documented hook events match `CURSOR_EVENTS` exactly, minus the two tab
  hooks (`beforeTabFileRead`, `afterTabFileEdit`) that we deliberately exclude

### Gemini CLI — `gemini`

Verified against [geminicli.com/docs/cli/skills](https://geminicli.com/docs/cli/skills/),
[geminicli.com/docs/core/subagents](https://geminicli.com/docs/core/subagents/),
[geminicli.com/docs/cli/custom-commands](https://geminicli.com/docs/cli/custom-commands/)
and [geminicli.com/docs/hooks/reference](https://geminicli.com/docs/hooks/reference/).

Correct: `GEMINI.md`, MCP in `.gemini/settings.json`, sub-agents
`.gemini/agents/*.md`, commands `.gemini/commands/*.toml` with a required
`prompt` field, skills at `.agents/skills/` (documented as the preferred alias,
higher precedence than `.gemini/skills/`).

Gemini CLI now has a hooks system. Eleven events:

```
BeforeTool  AfterTool  BeforeAgent  AfterAgent  BeforeModel
BeforeToolSelection  AfterModel  SessionStart  SessionEnd
Notification  PreCompress
```

Config lives under a `hooks` key in `settings.json`, the same file we already
merge MCP servers into. Shape is `{ matcher, sequential, hooks: [{ type,
command, name?, timeout?, description? }] }`, with `timeout` in milliseconds
rather than seconds.

We declare `hooks: false` for Gemini. Implementing this is a merge into a file
we already own a section of, so it is the least expensive of the four new hook
integrations.

### GitHub Copilot — `copilot`

Verified against
[code.visualstudio.com/docs/copilot/customization/overview](https://code.visualstudio.com/docs/copilot/customization/overview),
[code.visualstudio.com/docs/agent-customization/agent-skills](https://code.visualstudio.com/docs/agent-customization/agent-skills)
and [code.visualstudio.com/docs/agent-customization/hooks](https://code.visualstudio.com/docs/agent-customization/hooks).

Correct: `.github/copilot-instructions.md`, MCP in `.vscode/mcp.json` under the
`servers` key, prompts at `.github/prompts/*.prompt.md`, skills read from
`.agents/skills/` (also `.github/skills/` and `.claude/skills/`).

Two features have appeared since we last looked.

Custom agents live at `.github/agents/*.agent.md`. Our `capabilities()` sets
`agents: false`, so the whole sub-agent surface is hidden for Copilot projects.
The file naming convention differs from every other agent we support — the
`.agent.md` double extension needs the same treatment `command_file_name` already
gives `.prompt.md`.

Hooks live at `.github/hooks/*.json`, and VS Code also reads
`.claude/settings.json`. Eight events:

```
SessionStart  UserPromptSubmit  PreToolUse  PostToolUse
PreCompact    SubagentStart     SubagentStop  Stop
```

All eight are a subset of Claude Code's names, so a Copilot hook writer can reuse
the Claude event vocabulary directly.

VS Code also walks up from each workspace folder to the repository root
collecting customisations, which matters for monorepo projects but does not
change what we write.

### OpenCode — `opencode`

Verified against [opencode.ai/docs/config](https://opencode.ai/docs/config/) and
[opencode.ai/docs/skills](https://opencode.ai/docs/skills/).

Correct: `AGENTS.md`, MCP under the `mcp` key with `type: local | remote`,
`command` as an array, `environment` rather than `env`. Directory names are
plural (`.opencode/agents/`, `.opencode/commands/`), which is what we write;
singular names survive only for backwards compatibility. Skills at
`.agents/skills/` are scanned, walking up to the git worktree root.

Hooks are not a documented OpenCode feature. `hooks: false` is correct.

One incidental defect, not an upstream change.
[write_mcp_config](../../src-tauri/src/agent/opencode.rs) builds
`json!({ "$schema": ..., "mcp": ... })` from scratch and writes it over
`opencode.json`. It never reads the existing file. A user who keeps `model`,
`permission`, `instructions` or `agent` settings in `opencode.json` loses them on
the next sync. Every other agent that writes into a shared file merges first.

### Zed — `zed`

Verified against [zed.dev/docs/ai/mcp](https://zed.dev/docs/ai/mcp),
[zed.dev/docs/ai/instructions](https://zed.dev/docs/ai/instructions),
[zed.dev/docs/ai/skills](https://zed.dev/docs/ai/skills) and
[zed.dev/docs/ai/agent-profiles](https://zed.dev/docs/ai/agent-profiles).

Correct: MCP under `context_servers` in `.zed/settings.json`, flat
`command`/`args`/`env` for local servers and `url`/`headers` for remote. Skills
at `.agents/skills/` — and only there, plus `~/.agents/skills/`. Zed does not
read `.claude/skills/` or `.zed/skills/`.

Two problems.

`agents_dir` returns `.zed/agents` at
[src-tauri/src/agent/zed.rs:194](../../src-tauri/src/agent/zed.rs), and
`capabilities()` leaves `agents: true` by default. No Zed documentation describes
a `.zed/agents` directory. Agent profiles are stored under `agent.profiles` in
`settings.json`, not as files on disk. External agents are ACP integrations
added through the UI. On the evidence available, sub-agent Markdown files written
to `.zed/agents/` are never read by anything, and the capability badge shown in
the UI promises a sync that does not work. This should be verified once more
against the Zed source before removal, but the documentation is unambiguous.

Instruction file precedence has shifted underneath us. Zed's documented order is
`.rules`, `.cursorrules`, `.windsurfrules`, `.clinerules`,
`.github/copilot-instructions.md`, `AGENT.md`, `AGENTS.md`, `CLAUDE.md`,
`GEMINI.md`, first match wins. `.rules` is now described as a compatibility file,
retained rather than recommended, following Zed's migration from Rules to Skills
and Instructions in v1.4.0. `AGENTS.md` is the documented instruction file.

Writing `.rules` still works, because `.rules` is first in precedence. But in a
project that also has Codex or Cursor, Automatic writes the same rule content
twice, and Zed reads the copy in `.rules` while ignoring `AGENTS.md`. Switching
Zed to `AGENTS.md` would collapse that duplication. It is a behaviour change for
existing projects, so it needs a migration path rather than a flag flip.

### Junie — `junie`

Verified against [junie.jetbrains.com/docs/agent-skills.html](https://junie.jetbrains.com/docs/agent-skills.html).
JetBrains has moved the Junie docs off `jetbrains.com/help/junie` to
`junie.jetbrains.com/docs`.

Correct, and confirmed again today: MCP at `.junie/mcp/mcp.json`, guidelines at
`.junie/AGENTS.md` or root `AGENTS.md`.

Skills are broken. Junie looks in exactly two places:
`<projectRoot>/.junie/skills/<skill-name>/` and `~/.junie/skills/<skill-name>/`.
It does not scan `.agents/skills/`. The docs are explicit that Junie can *detect*
skills in `.cursor/skills/`, `.claude/skills/` and `.codex/skills/` only in order
to offer to import them into `.junie/skills/`.

`skill_dirs()` at [src-tauri/src/agent/junie.rs:44](../../src-tauri/src/agent/junie.rs)
correctly lists `.junie/skills` first. But `sync_skills()` at line 122 writes to
`.agents/skills/` regardless. Every skill Automatic syncs to a Junie project
lands somewhere Junie will not look. The fix is a one-line change to match the
pattern Kiro and Cline already use.

Custom agents exist for Junie through its extension format — an extension may
carry an `agents/` directory alongside `skills/`, `guidelines/` and `mcp/`. That
is a different packaging model from a plain agents directory, so `agents: false`
is defensible for now. It should be re-examined rather than assumed.

### Kiro — `kiro`

Verified against [kiro.dev/docs/mcp/configuration](https://kiro.dev/docs/mcp/configuration/),
[kiro.dev/docs/skills](https://kiro.dev/docs/skills/) and
[kiro.dev/docs/cli/custom-agents/configuration-reference](https://kiro.dev/docs/cli/custom-agents/configuration-reference/).

Correct: MCP at `.kiro/settings/mcp.json` under `mcpServers`, merged with
`~/.kiro/settings/mcp.json` and taking precedence. Skills at `.kiro/skills/`,
which is what we write. `AGENTS.md` in the workspace root is picked up
automatically as a steering directive.

Kiro CLI now supports custom agents as JSON files in `.kiro/agents/` (project)
and `~/.kiro/agents/` (global). The filename without `.json` is the agent id.
Fields include `name`, `description`, `prompt` (inline or a `file://` URI),
`mcpServers`, `tools`, `toolAliases`, `allowedTools`, `toolsSettings`,
`resources`, `hooks`, `includeMcpJson`, `model`, `keyboardShortcut`,
`welcomeMessage`. We set `agents: false`.

This is the only agent in the set whose sub-agent format is JSON rather than
Markdown-with-frontmatter or TOML, so it needs a `convert_agent_content` override
like Codex has, plus `agents_file_ext` returning `"json"`.

Kiro also has agent hooks in `.kiro/hooks/`, but these are file-system event
automations — they fire when files are created, saved or deleted, not on agent
lifecycle events. That is a different model from the `PreToolUse`-style hooks
Automatic's hook library represents. Worth a design decision before committing to
it, rather than a straight port.

MCP fields we do not write: `oauth`, `disabled`, `autoApprove`, `disabledTools`.

### Kilo Code — `kilo`

Verified against [kilo.ai/docs/code-with-ai/platforms/cli](https://kilo.ai/docs/code-with-ai/platforms/cli)
and [kilo.ai/docs/automate/mcp/using-in-kilo-code](https://kilo.ai/docs/automate/mcp/using-in-kilo-code).

This is the most serious finding. Kilo Code has rebranded to Kilo, moved its
documentation from `kilocode.ai` to `kilo.ai`, and rebuilt its configuration
around an OpenCode-derived format.

Current upstream state:

- Global config: `~/.config/kilo/kilo.json[c]`
- Project config: `./kilo.json[c]` or `./.kilo/kilo.json[c]`
- MCP under a top-level `mcp` key, with `type: "local" | "remote"`, `command` as
  an array, `environment` rather than `env`, plus `enabled` and `timeout`
- `AGENTS.md` at the repository root, created by `/init`
- Project directories under `.kilo/` — `agents`, `rules`

The documentation states that `./.kilocode/` and `./opencode.json[c]` are no
longer supported, and that Kilo no longer falls back to configuration in
`.opencode` directories.

Our implementation writes `.kilocode/mcp.json` with an `mcpServers` key and
`command`/`args`/`env`. Kilo does not read that path, and would not understand
the shape if it did. Detection in `detect_in` also keys off `.kilocode/`, so
existing Kilo projects are no longer recognised either.

The new format is byte-for-byte the shape our OpenCode writer already produces.
Porting is mostly a matter of changing the filename, the `$schema` value, and the
directory constants — the transport mapping can be shared.

Kilo's CLI documentation explicitly refers readers to OpenCode's config
documentation, which suggests Kilo will continue tracking OpenCode. Sharing the
writer rather than copying it would be the better call.

### Droid — `droid`

Verified against [docs.factory.ai/cli/configuration/mcp](https://docs.factory.ai/cli/configuration/mcp),
[docs.factory.ai/cli/configuration/custom-droids](https://docs.factory.ai/cli/configuration/custom-droids),
[docs.factory.ai/cli/configuration/skills](https://docs.factory.ai/cli/configuration/skills)
and [docs.factory.ai/cli/configuration/hooks-guide](https://docs.factory.ai/cli/configuration/hooks-guide).

Correct: MCP at `.factory/mcp.json` under `mcpServers` with
`type: stdio | http | sse`, `AGENTS.md` for instructions, skills read from
`.agents/skills/` and `.agent/skills/` in compatibility mode alongside
`.factory/skills/`.

Custom droids are Markdown files with YAML frontmatter in `.factory/droids/`
(project) and `~/.factory/droids/` (user). Frontmatter is `name`, `description`,
`model`, `tools`, `reasoningEffort`, `mcpServers`. The body is the system prompt
and cannot be empty. This is close enough to Claude Code's sub-agent shape that
the default `convert_agent_content` pass-through would mostly work; only the
directory name (`droids`, not `agents`) differs. We set `agents: false`.

Hooks live in `.factory/hooks.json` (project) or `~/.factory/hooks.json` (user),
with `.factory/hooks/hooks.json` as a legacy path. Nine events:

```
PreToolUse  PostToolUse  UserPromptSubmit  Notification  Stop
SubagentStop  PreCompact  SessionStart  SessionEnd
```

The JSON shape matches Claude Code's, with one addition: an optional
`commandRegex` alongside `matcher`. We declare `hooks: false`.

Factory also warns that project-level `.factory/mcp.json` is committed to the
repository and must not contain header auth tokens, `oauth.clientSecret` or API
keys. Our inherited-environment mechanism already avoids writing secrets, which
is the right behaviour here.

### Goose — `goose`

Verified against [goose-docs.ai/docs/guides/config-files](https://goose-docs.ai/docs/guides/config-files/)
and [goose-docs.ai/docs/guides/context-engineering/using-skills](https://goose-docs.ai/docs/guides/context-engineering/using-skills/).
Goose docs have moved from `block.github.io/goose` to `goose-docs.ai`.

No gaps. Every capability decision we made still holds.

- Config is global only: `~/.config/goose/config.yaml`, with `permission.yaml`
  and `secrets.yaml` beside it. There is no per-repository config file, so
  `mcp_servers: false` remains correct
- Context files are `AGENTS.md` and `.goosehints`, both looked for at each level
  of the working directory hierarchy and combined. `AGENTS.md` is our choice and
  is correct
- Skills come from `~/.agents/skills/` and `.agents/skills/`, described as the
  recommended standard, with `.goose/skills/` and `.claude/skills/` kept for
  backwards compatibility. We write the recommended path
- Sub-agents are spawned as processes from within a session and from instructions
  inside a `SKILL.md`. There is still no persona directory on disk, so
  `agents: false` remains correct. This matches the reasoning already recorded in
  the `goose_recipes_not_subagents` memory

### Warp — `warp`

Verified against [docs.warp.dev/knowledge-and-collaboration/rules](https://docs.warp.dev/knowledge-and-collaboration/rules).

Correct: `AGENTS.md` is Warp's default project rules file, and `mcp_servers:
false` still holds because there is no documented project-level MCP config file.

One behaviour to fix. Warp's documentation states that if both `WARP.md` and
`AGENTS.md` exist in the same directory, `WARP.md` takes priority. Our
`detect_in` matches on legacy `WARP.md`, so a project with a pre-existing
`WARP.md` is correctly detected as a Warp project — and then Automatic writes its
content to `AGENTS.md`, which Warp ignores in favour of the stale `WARP.md`
sitting next to it. The user sees a successful sync and no effect.

Either migrate `WARP.md` on first sync the way `.cursorrules` is handled, or
surface a drift warning. Silently writing the losing file is the one option that
should not stay.

Separately, `owned_config_paths` returns `AGENTS.md` and `WARP.md` at
[src-tauri/src/agent/warp.rs:93](../../src-tauri/src/agent/warp.rs). The default
`cleanup_mcp_config` deletes every path in that list. `AGENTS.md` is shared with
Codex, Cursor, Kilo, Droid, Kiro, OpenCode and Goose. Nothing calls
`cleanup_mcp_config` in this crate today, so no data is being lost, but if the
CLI wires it up during the 2.0 split, removing Warp from a project would delete
the instruction file every other agent depends on. `owned_config_paths` is
documented as MCP config files that Automatic exclusively owns; an instruction
file shared with seven other agents does not qualify.

### Cline — `cline`

Verified against [docs.cline.bot/features/cline-rules](https://docs.cline.bot/features/cline-rules),
[docs.cline.bot/features/skills](https://docs.cline.bot/features/skills) and
[docs.cline.bot/mcp/configuring-mcp-servers](https://docs.cline.bot/mcp/configuring-mcp-servers).

Correct: rules are a directory of `.md` and `.txt` files at `.clinerules/`, all
combined, so writing `.clinerules/automatic.md` is right. Skills come from
`.cline/skills/`, `.clinerules/skills/` and `.claude/skills/` — notably *not*
`.agents/skills/`, so our decision to special-case Cline's skill path is correct
and must not be normalised away.

Still no project-level MCP config, so `mcp_servers: false` holds. But the Cline
CLI now reads `~/.cline/mcp.json`. We implement no `discover_global_mcp_servers`
for Cline, so those servers are invisible during first-run import. Adding it is
low-risk: the file is standard `mcpServers` JSON and discovery is read-only.

Cline also reads `AGENTS.md` for cross-tool compatibility, which we do not need
to act on.

### Antigravity — `antigravity`

Verified against
[ai.google.dev/gemini-api/docs/antigravity-agent](https://ai.google.dev/gemini-api/docs/antigravity-agent)
plus the Google Cloud Community write-ups that document the on-disk paths, which
the official docs still do not.

Correct: skills at `.agents/skills/`, no project-level MCP config, so
`mcp_servers: false` holds.

The open TODO in the module header can be closed. MCP configuration for both
Antigravity IDE and Antigravity CLI is read from a single shared global file:

```
~/.gemini/config/mcp_config.json
```

That is enough to implement `discover_global_mcp_servers` for Antigravity, which
would let first-run import pick up servers the user already has. There is still
no project-scoped file, so `write_mcp_config` stays a no-op and the `mcp_note`
stays accurate. One caveat recorded by the same source: environment variable
interpolation does not work in that file, so values are hardcoded. Automatic
should not write it even if it could.

Two doc-comment corrections. Global skills are at `~/.gemini/config/skills/`,
not `~/.gemini/antigravity/skills/` as the header claims. Built-in skills ship
from `~/.gemini/antigravity/builtin/skills/` and
`~/.gemini/antigravity-cli/builtin/skills/`.

An Antigravity CLI now exists alongside the IDE and shares the same harness and
config. Our single `antigravity` agent covers both, which is the right modelling.

The instruction file question is unresolved. Our header states Antigravity reads
`GEMINI.md` and not `AGENTS.md`, attributed to community testing. Google's own
Gemini API documentation for the Antigravity agent now refers to mounting
`AGENTS.md` for instructions, and Google's codelab material is built around
`AGENTS.md`. These may describe the hosted agent rather than the local IDE. This
needs a first-hand check before changing `project_file_name`, because getting it
wrong breaks every Antigravity project silently.

### Pi — `pi`

Verified against [pi.dev](https://pi.dev/) and the published package
`@earendil-works/pi-coding-agent`.

Correct: `AGENTS.md` for instructions, `.pi/skills/` for skills, `.pi/agents/`
for sub-agents, and MCP through a community extension rather than core. Pi
deliberately ships without MCP, sub-agents, plan mode or permission popups in
core; each is an installable extension. Our `config_description` of
`.pi/mcp.json (via pi-mcp-adapter)` states that dependency honestly.

One unclaimed surface: `.pi/prompts/` is a documented Pi primitive alongside
`.pi/agents/` and `.pi/skills/`. We set `commands: false`. Whether Pi's prompts
directory maps onto Automatic's commands model depends on which extension is
installed, so this needs a decision rather than an implementation.

Note that Pi has moved under Earendil Inc. The harness itself is unchanged.

## Documentation URLs that moved

Source comments and the `Hooks.tsx` header cite URLs that now redirect. Worth
updating so the next audit starts from live pages.

| Old | Current |
|---|---|
| `docs.claude.com/en/docs/claude-code/*` | `code.claude.com/docs/en/*` |
| `developers.openai.com/codex/*` | `learn.chatgpt.com/docs/*` |
| `kilocode.ai/docs/*` | `kilo.ai/docs/*` |
| `jetbrains.com/help/junie/*` | `junie.jetbrains.com/docs/*` |
| `block.github.io/goose/*` | `goose-docs.ai/docs/*` |

## Open questions

These need a first-hand check, not another documentation read.

1. Does Antigravity read `AGENTS.md`? Google's API docs now say yes; our code
   comment says community testing said no. Test before changing anything.
2. Does Zed read anything from `.zed/agents/`? The documentation says agent
   profiles live in `settings.json` and says nothing about the directory.
   Confirm against the Zed source before removing the capability.
3. Should Kiro's file-event hooks and Pi's `.pi/prompts/` map onto Automatic's
   hook and command models at all? Both are structurally different from the
   lifecycle-hook and slash-command shapes the library assumes.

## Not covered

This audit checked the 16 agents already implemented. It did not survey agents
Automatic does not yet support. Several appeared repeatedly in vendor
compatibility tables while researching the above — Copilot CLI, Junie CLI and
Kiro CLI are distinct products from their IDE counterparts with their own config
paths, and Roo Code appears to have shut down. Whether any of those warrant their
own `Agent` implementation is a separate scoping question.
