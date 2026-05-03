# Multi-agent support in Settings > Agents

## Context

Settings > Agents currently supports one agent (Claude / Anthropic). The plan is to add five more model providers — GitHub Models (the user's "GitHub Copilot"), Cloudflare Workers AI, Z.ai, OpenCode Zen, OpenAI — plus Cloudflare AI Gateway as a routing option that wraps any of those.

Today every in-app AI feature (file generation, insight generation, recommendations, AI Playground) goes through `src-tauri/src/core/ai.rs`, which hardcodes the Anthropic Messages API. Credentials live in a single OS-keychain entry keyed `anthropic`. To support multiple agents we need an abstraction layer underneath — one that lets each agent describe its credentials, expose the four operations our consumers need (chat / structured chat / chat-with-tools / list-models), and translate between Anthropic-shaped tool blocks and OpenAI-shaped tool calls without touching consumer code.

The plan is staged: a foundation refactor (no user-visible change) lands first, then each agent ships in its own PR on top of that foundation.

## Outcome

After all PRs:

- Settings > Agents shows one card per supported agent. Each card collects whatever credentials its provider needs (single token for most; token + account ID for Workers AI).
- A single "active agent" setting controls which agent serves all in-app AI features. The existing `agent_features_enabled` master toggle continues to gate everything.
- Every supported agent can run the recommendations tool-loop, project file generation, and the AI Playground.
- Anthropic and Workers AI agents have an optional "route through Cloudflare AI Gateway" sub-config (account ID + gateway ID + optional CF token).

---

## PR 1 — Foundation (no user-visible change)

Goal: extract a per-agent abstraction without changing behaviour. Anthropic remains the only configured agent; existing users see no difference.

### New module: `src-tauri/src/core/agents/`

```
core/agents/
  mod.rs           # AgentId, AgentRegistry, public API
  credential.rs    # AgentCredential enum + keychain helpers
  message.rs       # provider-neutral Message + Tool + ToolCall types
  client.rs        # AgentClient trait
  clients/
    anthropic.rs   # impl AgentClient for Anthropic (extracted from core/ai.rs)
    openai_compat.rs  # impl AgentClient for OpenAI-style /chat/completions
```

Key types:

- `AgentId` — string-newtype (`"anthropic"`, `"openai"`, `"github-models"`, `"workers-ai"`, `"zai"`, `"opencode-zen"`).
- `AgentCredential` — enum covering the three shapes we'll need:
  - `SingleToken(String)` — Anthropic, OpenAI, GitHub Models, Z.ai, OpenCode Zen
  - `TokenAndAccount { token: String, account_id: String }` — Workers AI
  - Plus an optional `gateway: Option<GatewayConfig>` field for the AI Gateway routing wrap (no extra variants needed).
- `AgentClient` trait — async methods `chat`, `chat_structured`, `chat_with_tools`, `list_models`, all taking provider-neutral `Message`/`Tool` types.
- `Message` — `{ role, content_blocks: Vec<ContentBlock> }` where `ContentBlock` is `Text | ToolUse { id, name, input } | ToolResult { tool_use_id, content }`. This is the **single biggest design decision** — pick a shape that captures both Anthropic's `tool_use`/`tool_result` blocks and OpenAI's `tool_calls`/`role:tool` messages. Each client adapter translates in/out.

### Credential storage

Keep using the OS keychain via `src-tauri/src/core/credentials.rs`, but key entries by `agent_id` instead of provider name. Compound credentials (Workers AI) serialise to a small JSON blob inside one keychain entry rather than two entries, to keep the abstraction one-key-per-agent.

Migrate the existing `anthropic` entry on first read so existing users keep their key (read both keys, prefer `anthropic` if present, write back under the new id, delete the old one).

### Settings additions

In `src-tauri/src/core/settings.rs`, add:

- `active_agent: Option<String>` — which agent powers in-app features. Defaults to `"anthropic"` if any anthropic key is stored, else first configured agent, else `None`.
- Per-agent gateway config (Anthropic + Workers AI only for now): `agent_gateways: HashMap<String, GatewayConfig>`.

Both `skip_serializing_if`'d to keep legacy `settings.json` clean (same pattern as `agent_features_enabled`).

### Refactor existing call sites

Touch only routing — no behaviour changes:

- `src-tauri/src/core/ai.rs` becomes a thin facade that delegates to `core::agents::active_client()`. Keep the existing `chat`, `chat_structured`, `chat_with_tools`, `chat_with_tools_returning_history`, `list_models`, `resolve_api_key`, `agent_features_enabled` public signatures unchanged so consumers don't move.
- `core::credentials::KNOWN_PROVIDERS` becomes `core::agents::registry::known_agents()` (used by the auto-toggle in `save_api_key` / `delete_api_key`).
- Tool definitions and executors (`read_file_tool_def`, `list_skills_tool_def`, …) stay where they are; they only ever produced JSON Schema for the `tools` array, which is provider-neutral. The Anthropic-format conversion lives in `clients::anthropic`; OpenAI format in `clients::openai_compat`.

### UI: keep current behaviour

The existing `src/pages/settings/Agents.tsx` keeps showing only Claude until subsequent PRs add cards. No change.

### Tests

- Unit tests in `core/agents/message.rs` for the Anthropic ↔ neutral and OpenAI ↔ neutral conversions, including a tool-use round trip.
- Migration test: `anthropic` keychain entry → `anthropic` agent id; settings without `active_agent` resolve to `"anthropic"` if a key exists.
- Existing tests must still pass.

---

## PRs 2–6 — One agent per PR

Each PR adds:

- An entry in the agent registry (id, label, credential shape, default model, base URL, builder).
- A card on `src/pages/settings/Agents.tsx` — driven by the credential variant so single-token agents are uniform and Workers AI gets a two-field card.
- The model list (curated static list per provider; `list_models` falls back to the static list when the provider has no listing endpoint).

### PR 2 — GitHub Models

- Provider: OpenAI-compatible
- Base URL: `https://models.github.ai/inference`
- Auth: `Authorization: Bearer {pat}` plus `X-GitHub-Api-Version: 2026-03-10`
- Credential: `SingleToken`
- Default model: `openai/gpt-4.1`
- Supports tools and `response_format: json_schema`
- Tightest tier limits — flag in UI copy: "free tier rate-limited; opt in to paid usage for production".

### PR 3 — OpenAI

- Provider: native OpenAI
- Base URL: `https://api.openai.com/v1`
- Auth: `Authorization: Bearer {key}`
- Credential: `SingleToken`
- Default model: `gpt-4o-mini` (or current best small model)
- Reuses `clients::openai_compat` unchanged.

### PR 4 — Z.ai

- Provider: OpenAI-compatible
- Base URL: `https://api.z.ai/api/paas/v4` (also has Anthropic-compat at `/api/anthropic` — not required, but worth noting we could route through `clients::anthropic` instead)
- Auth: `Authorization: Bearer {key}`
- Credential: `SingleToken`
- Default model: `glm-4.7`

### PR 5 — OpenCode Zen

- Provider: OpenAI-compatible (the `@opencode-ai/sdk/v2` route)
- Base URL: TBD — needs confirmation from `https://opencode.ai/docs/providers/`. If their HTTP server has an OpenAI-compat endpoint, point at it.
- Auth: `Authorization: Bearer {key}`
- Credential: `SingleToken`

### PR 6 — Cloudflare Workers AI

- Provider: OpenAI-compatible (`/ai/v1/chat/completions`)
- Base URL: `https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1`
- Auth: `Authorization: Bearer {token}` (token needs Workers AI Read+Edit)
- Credential: `TokenAndAccount`
- Default model: `@cf/meta/llama-3.1-8b-instruct`
- **Risk**: not every Workers AI model supports tool calling — recommendations may not work on all models. Validate with the default before merging; document model picker.

---

## PR 7 — Cloudflare AI Gateway routing

Not a new agent. A per-agent option that rewrites the request URL and adds optional CF auth.

- Settings UI: "Route through Cloudflare AI Gateway" expander on Anthropic and Workers AI cards. Captures `account_id`, `gateway_id`, optional `cf_aig_token`.
- Backend: `clients::anthropic` and `clients::openai_compat` accept an optional `GatewayConfig` and rewrite the base URL to `https://gateway.ai.cloudflare.com/v1/{account_id}/{gateway_id}/{provider}/...` plus the `cf-aig-authorization` header when set.
- Universal/compat AI Gateway endpoint not needed — we already have per-provider clients.

---

## Critical files modified across the work

- `src-tauri/src/core/ai.rs` — becomes facade in PR 1
- `src-tauri/src/core/credentials.rs` — keys by agent id; migration for legacy `anthropic` entry
- `src-tauri/src/core/settings.rs` — `active_agent`, `agent_gateways`
- `src-tauri/src/core/agents/` — new module (PR 1)
- `src-tauri/src/commands/credentials.rs` — `agent_features_enabled`, `save_api_key`, `delete_api_key`, `has_api_key` adapt to agent-id-keyed storage
- `src-tauri/src/commands/ai.rs` / `recommendations.rs` / `projects.rs` / `project_files.rs` — no signature change; transitively use the active client
- `src-tauri/src/lib.rs` — register any new commands (likely `list_agents_v2` for the registry, `set_active_agent`)
- `src/pages/settings/Agents.tsx` — card per agent, active-agent selector, gateway sub-config
- `src/pages/utilities/AiPlayground.tsx` — model picker reads from active agent's model list
- `src/pages/workspace/Projects.tsx` — no logic change; tooltip stays "Enable Agent features to access"

## Existing utilities to reuse

- `src-tauri/src/core/credentials.rs` `Entry`-based keychain primitives — extend, don't replace.
- `src-tauri/src/core/settings.rs` `read_settings` / `write_settings` — already typed, just add fields.
- The `agent_features_enabled` toggle and its auto-flip — generalise the "any known key stored" check; keep semantics.
- All tool definitions (`read_file_tool_def`, `list_skills_tool_def`, …) and executors in `core/ai.rs` — provider-neutral, kept as-is.

## Verification

### PR 1 (foundation)

- `make check` (TypeScript + cargo) clean.
- `cargo test` all existing tests pass; new tests for message conversion + credential migration pass.
- Manual: install build, fire up Settings > Agents, save a Claude key, run AI Playground, run Recommendations on a project, generate a `CLAUDE.md`. All four behave identically to current `main`.
- Inspect `~/.agents/settings.json` — no `active_agent` written until the user changes it (default value omitted).
- Existing user with stored Anthropic keychain entry: app opens, AI features still work, no re-prompt for key.

### Per-agent PRs (2–6)

- Same cycle as PR 1, plus:
- Add the agent's key in Settings > Agents, set as active agent, repeat the four flows above.
- For Workers AI: confirm the chosen default model completes the recommendations tool-loop end-to-end.
- Run `cargo clippy -- -D warnings` after each PR.

### PR 7 (AI Gateway)

- With Anthropic agent active, enable gateway routing with a real `account_id` + `gateway_id`. Run AI Playground. Verify the request URL in network logs is the gateway URL, not `api.anthropic.com`.
- Same for Workers AI agent.

## Out of scope

- Per-feature agent selection (recommendations on Agent X, file gen on Agent Y) — single active-agent setting only. Revisit if there's demand.
- Streaming responses — no consumer needs it today.
- Embeddings — no consumer needs it.
- Reverse-engineered Copilot OAuth flow.
- AI Gateway's Universal/OpenAI-compat endpoint — superseded by per-provider routing.
