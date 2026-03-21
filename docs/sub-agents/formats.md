# Coding Agent Sub-Agent Format Reference

> Last updated: March 2026. Covers file formats, storage locations, frontmatter schemas, and invocation patterns for each major coding agent's sub-agent system.

---

## Claude Code

**Format:** Markdown with YAML frontmatter  
**Invocation tool:** `Agent()` (formerly `Task()`, alias still works)  
**Max concurrent:** 10  
**Nesting:** Sub-agents cannot spawn sub-agents. Use Agent Teams for multi-session coordination.

### Storage Locations

| Scope | Path | Priority |
|-------|------|----------|
| Project | `.claude/agents/*.md` | Highest (wins on name collision) |
| User | `~/.claude/agents/*.md` | Lower |

### File Format

```markdown
---
name: code-reviewer
description: Reviews code for quality and best practices. Use PROACTIVELY after code changes.
tools: Read, Glob, Grep, Bash
model: sonnet
color: yellow
---

You are a code reviewer. When invoked, analyze the code and provide
specific, actionable feedback on quality, security, and best practices.
```

### Frontmatter Fields

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `name` | Yes | string | Identifier. Overrides filename if set. |
| `description` | Yes | string | Used by Claude to decide when to delegate. Include "PROACTIVELY" to enable auto-delegation. |
| `tools` | No | comma-separated string | Whitelist of tools. Omit to inherit parent session tools. Options: `Read`, `Write`, `Edit`, `Bash`, `Glob`, `Grep`, `MultiEdit`, `Notebook`, `TodoRead`, `TodoWrite`, `WebFetch`, `WebSearch`, MCP tool names. |
| `model` | No | string | `sonnet`, `opus`, `haiku`, `inherit`, or a specific model ID. |
| `color` | No | string | Display colour in terminal UI. |
| `hooks` | No | object | Lifecycle hooks (e.g., `PreToolUse`, `SubagentStop`). |

### Invocation

```
# Automatic: Claude reads description and delegates when it matches
# Explicit in prompt:
Use the code-reviewer subagent to review the auth module.

# CLI (JSON agents):
claude --agents '{"reviewer": {"description": "Reviews code", "prompt": "You are a code reviewer"}}'

# SDK (programmatic):
query({ agents: { reviewer: { description: "...", prompt: "..." } } })
```

### Agent Teams (multi-session, inter-agent messaging)

Agent Teams are distinct from sub-agents. They coordinate across separate sessions with shared task lists and bidirectional messaging via inbox files at `~/.claude/<teamName>/inboxes/<agentName>.json`.

---

## OpenAI Codex

**Format:** TOML files  
**Invocation:** Explicit prompt instruction (Codex does not auto-spawn)  
**Max concurrent threads:** 6 (configurable via `agents.max_threads`)  
**Max nesting depth:** 1 (configurable via `agents.max_depth`)

### Storage Locations

| Scope | Path |
|-------|------|
| Project | `.codex/agents/*.toml` |
| User | `~/.codex/agents/*.toml` |

### File Format

```toml
name = "reviewer"
description = "PR reviewer focused on correctness, security, and missing tests."
model = "gpt-5.4"
model_reasoning_effort = "high"
sandbox_mode = "read-only"

nickname_candidates = ["Athena", "Ada"]

developer_instructions = """
Review code like an owner.

Check for correctness, security risks, and missing test coverage.
Provide prioritised, actionable findings.
"""
```

### TOML Fields

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `name` | Yes | string | Agent identifier. Source of truth (filename is convention only). |
| `description` | Yes | string | Shown to Codex when choosing agents. |
| `developer_instructions` | No | string (multiline) | System prompt / instructions. |
| `model` | No | string | `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.3-codex-spark`, etc. Inherits parent if omitted. |
| `model_reasoning_effort` | No | string | `low`, `medium`, `high`. |
| `sandbox_mode` | No | string | `read-only`, `workspace-write`, etc. |
| `nickname_candidates` | No | array of strings | Display names for parallel instances. |
| `mcp_servers` | No | table | MCP server configuration. |
| `skills.config` | No | array | Skill overrides. |

### Config-Level Agent Roles

Roles can also be defined in `config.toml` rather than standalone files:

```toml
# In .codex/config.toml or ~/.codex/config.toml
[agents]
max_threads = 6
max_depth = 1
job_max_runtime_seconds = 1800

[agents.reviewer]
description = "Find correctness, security, and test risks in code."
config_file = "./agents/reviewer.toml"  # relative to config.toml
nickname_candidates = ["Athena", "Ada"]
```

### Instructions Layer (AGENTS.md)

Codex also reads `AGENTS.md` files for general instructions (not sub-agents per se, but context injection). Discovery order per directory: `AGENTS.override.md` → `AGENTS.md` → fallback names.

| Scope | Path |
|-------|------|
| Global | `~/.codex/AGENTS.md` |
| Project | `AGENTS.md` in repo root and subdirectories |

### Invocation

```
# Must be explicit in prompt:
Spawn one agent for security risks, one for test gaps, and one for maintainability.

# Named agent reference:
Have browser_debugger reproduce it, code_mapper trace the responsible code path,
and ui_fixer implement the smallest fix.
```

---

## Cursor

**Format:** Markdown with YAML frontmatter  
**Invocation:** Automatic (description-based delegation) or explicit prompt  
**Background agents:** Use git worktrees for isolation

### Storage Locations

| Scope | Path |
|-------|------|
| Project | `.cursor/agents/*.md` |
| User | `~/.cursor/agents/*.md` |
| Background state | `~/.cursor/subagents/` |

### File Format

```markdown
---
name: security-auditor
description: Reviews code changes for security vulnerabilities. Use when new endpoints, auth logic, or data handling are added.
model: inherit
readonly: true
is_background: false
---

You are a security auditor. When invoked:

1. Identify the changed files via git diff
2. Scan for OWASP Top 10 vulnerabilities
3. Check input validation and auth boundaries
4. Report findings with file path, line number, and severity
```

### Frontmatter Fields

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `name` | Yes | string | Identifier; filename also serves as identity. |
| `description` | Yes | string | Read by main agent to decide delegation. Be specific about trigger conditions. |
| `model` | No | string | `inherit` (parent model), `fast` (cheaper/faster), or specific model ID. |
| `readonly` | No | boolean | `true` restricts to read-only operations. |
| `is_background` | No | boolean | `true` makes it async; parent doesn't wait. Uses git worktrees. |

### Rules Layer (.cursor/rules/)

Cursor also has a rules system (separate from sub-agents) using MDC (Markdown Configuration) files:

```markdown
---
globs:
  - "apps/web/**"
  - "packages/ui/**"
description: "Frontend-specific rules"
---

# Frontend Rules
- App Router only
- Use Server Components by default
```

Rules live in `.cursor/rules/*.md` and apply contextually based on glob patterns.

### Skills Layer

Cursor also supports `SKILL.md` manifests via the Skills Marketplace, same format as the cross-agent Agent Skills standard.

---

## Gemini CLI

**Format:** Markdown with YAML frontmatter (current) or TOML (experimental/legacy)  
**Invocation:** Automatic (description-based routing) or `@agent_name` syntax  
**Nesting:** Not supported natively

### Storage Locations

| Scope | Path | Alias |
|-------|------|-------|
| Project | `.gemini/agents/*.md` | `.agents/agents/*.md` |
| User | `~/.gemini/agents/*.md` | `~/.agents/agents/*.md` |

### File Format (Markdown — current)

```markdown
---
name: security-reviewer
description: Reviews code for security vulnerabilities. Use when authentication, authorization, or data handling code is modified.
tools:
  - read_file
  - run_shell_command
  - google_web_search
---

You are a security review specialist. When invoked:

1. Identify modified files using git diff
2. Scan for common vulnerabilities
3. Return findings as structured JSON
```

### File Format (TOML — experimental)

```toml
name = "crypto_bro"
display_name = "Crypto Data Agent 📊"
description = "Fetches crypto price and news, returning structured JSON."
tools = ["run_shell_command"]

[prompts]
system_prompt = """
You are a specialized Crypto Data Agent.
Fetch real-time price and news for the given cryptocurrency.
Output structured JSON.
"""
```

### Frontmatter Fields (Markdown)

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `name` | Yes | string | Agent identifier. |
| `description` | Yes | string | Used for routing decisions by main agent. |
| `tools` | No | array of strings | Tool whitelist. Uses Gemini tool names: `read_file`, `write_file`, `run_shell_command`, `google_web_search`, `write_todos`. |

### Settings Configuration

Enable sub-agents in `~/.gemini/settings.json` or `.gemini/settings.json`:

```json
{
  "experimental": {
    "enableAgents": true
  }
}
```

Override built-in agent settings:

```json
{
  "agents": {
    "overrides": {
      "codebase_investigator": {
        "model": "gemini-3-pro",
        "maxTurns": 20
      }
    }
  }
}
```

### Built-in Agents

| Agent | Purpose | Default |
|-------|---------|---------|
| `codebase_investigator` | Deep code analysis and reverse engineering | Enabled |
| `cli_help` / `introspection` | Help with Gemini CLI usage | Enabled |
| `agent_router` | Routes tasks to appropriate sub-agent | Enabled |
| `browser_agent` | Web automation via accessibility tree | Experimental |

### Invocation

```
# Automatic: main agent routes based on description match
# Explicit with @ syntax:
@codebase_investigator Map out the auth system architecture.

# Management commands:
/agents reload
/agents enable <agent_name>
/agents disable <agent_name>
```

### Context Files (GEMINI.md)

General instructions (not sub-agents) via `GEMINI.md` in project root. Auto-included in every prompt.

### Remote Sub-agents (A2A Protocol)

Gemini CLI also supports remote agents via the Agent-to-Agent protocol, configured in settings.json with authentication (API key, OAuth2, or Google ADC).

---

## Cline

**Format:** File-based agent configs (Markdown with YAML frontmatter, recent addition)  
**Invocation:** Explicit prompt only (Cline does not auto-delegate)  
**Capabilities:** Read-only research agents. Cannot write files, use browser, or access MCP.  
**Nesting:** Sub-agents cannot spawn sub-agents.

### Storage Locations

| Scope | Path |
|-------|------|
| Project rules | `.clinerules/*.md` |
| Agent configs | File-based via `AgentConfigLoader` (v3.67+) |

### Sub-agent Capabilities

Cline's sub-agents are more constrained than other tools:

| Can Do | Cannot Do |
|--------|-----------|
| Read files | Write/edit files |
| Search code (`rg`, `grep`) | Use browser |
| List directories | Access MCP servers |
| Run read-only commands | Spawn nested sub-agents |
| Use skills | Web search |

### Invocation

```
# Must be explicit in prompt:
Use subagents to explore how authentication works and where the database models are defined.

# Enable in settings:
# Cline Settings → Agent section → toggle Subagents on
```

### Rules Layer (.clinerules/)

Cline's primary configuration system. Markdown files for project guidelines:

```
.clinerules/
├── coding-standards.md
├── testing-patterns.md
└── security-guidelines.md
```

### AGENTS.md Support

As of late 2025, there's an open proposal (Issue #5033) for AGENTS.md support alongside `.clinerules/`. Status: under discussion.

---

## OpenCode

**Format:** Markdown files or JSON configuration  
**Invocation:** `@agent_name` mention or automatic via Task tool  
**Differentiator:** Supports mixing models from different providers in the same team.

### Storage Locations

| Scope | Path |
|-------|------|
| Project | `.opencode/agents/*.md` |
| User | `~/.config/opencode/agents/*.md` |
| Config | `opencode.json` |

### File Format (Markdown)

```markdown
---
description: Code review specialist
mode: subagent
color: "#FF5733"
permission:
  task:
    - pattern: "*"
      access: deny
    - pattern: "orchestrator-*"
      access: allow
hidden: false
---

You are in code review mode. Focus on:

- Security vulnerabilities
- Performance issues
- Code clarity

Provide constructive feedback without making direct changes.
```

The filename becomes the agent name (e.g., `review.md` → `review` agent).

### JSON Configuration

```json
{
  "agents": {
    "review": {
      "description": "Code review specialist",
      "mode": "subagent",
      "model": "claude-sonnet-4",
      "color": "#FF5733",
      "permission": {
        "task": [
          { "pattern": "*", "access": "deny" },
          { "pattern": "orchestrator-*", "access": "allow" }
        ]
      }
    }
  }
}
```

### Configuration Fields

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `description` | Yes | string | Agent purpose description. |
| `mode` | No | string | `primary`, `subagent`, or `all` (default). |
| `model` | No | string | Any supported model from any provider. |
| `color` | No | string | Hex colour or theme name (`primary`, `accent`, `error`, etc.). |
| `hidden` | No | boolean | Hide from `@` autocomplete; still invocable by Task tool. |
| `permission.task` | No | array | Glob-based rules for which sub-agents this agent can invoke. Last matching rule wins. |
| `top_p` | No | float | Response diversity (0.0–1.0). |
| `temperature` | No | float | Sampling temperature. |

### Agent Teams (peer-to-peer messaging)

OpenCode's team implementation allows full peer-to-peer messaging between agents (not just hub-and-spoke like Claude Code). Any teammate can `team_message` any other by name.

---

## OpenClaw

**Format:** Configuration-based  
**Invocation:** `/subagents spawn` command or `sessions_spawn` tool  
**Nesting:** Configurable depth

### Invocation

```
/subagents spawn <agentId> <task> [--model <model>] [--thinking <level>]
/subagents info    # show run metadata
/focus <label>     # switch to subagent thread
```

### Key Features

- Session isolation: each sub-agent runs in `agent:<agentId>:subagent:<uuid>`
- Announce-on-completion: results posted back to requester chat channel
- Configurable nesting depth
- Model and thinking level overrides per sub-agent
- Thread binding for persistent sessions

---

## VS Code (Native, v1.109+)

VS Code 1.109 introduced native multi-agent support running Claude, Codex, and Copilot agents. Sub-agents run in parallel with visibility into task status, agent type, and expandable prompts/results.

Custom agents defined via `.vscode/agents/` directory, using the same format as the underlying agent (Claude or Codex).

---

## Cross-Agent Standards

### AGENTS.md (Linux Foundation / Agentic AI Foundation)

An open format for general agent instructions. Not a sub-agent definition, but project-level context. Supported by: Codex, Cursor, Jules, Factory, Aider, goose, OpenCode, Zed, Warp, VS Code, Devin.

```markdown
# AGENTS.md

## Dev environment tips
- Use `pnpm dlx turbo run where <project_name>` to jump to a package

## Testing instructions
- Run `pnpm turbo run test --filter <project_name>`
```

### Agent Skills (Open Standard)

Shared skill format across tools. A skill is a directory with `SKILL.md` plus optional scripts and references.

| Tool | Skill Location |
|------|---------------|
| Claude Code | `.claude/skills/` or `~/.claude/skills/` |
| Codex | `.agents/skills/` or `~/.codex/skills/` |
| Cursor | `.cursor/skills/` |
| Gemini CLI | `.gemini/skills/` or `.agents/skills/` |
| Cline | `.clinerules/` (transitioning) |

---

## Format Comparison Matrix

| Feature | Claude Code | Codex | Cursor | Gemini CLI | Cline | OpenCode |
|---------|------------|-------|--------|-----------|-------|----------|
| **File format** | MD + YAML | TOML | MD + YAML | MD + YAML | Config-based | MD + YAML / JSON |
| **Project path** | `.claude/agents/` | `.codex/agents/` | `.cursor/agents/` | `.gemini/agents/` | `.clinerules/` | `.opencode/agents/` |
| **User path** | `~/.claude/agents/` | `~/.codex/agents/` | `~/.cursor/agents/` | `~/.gemini/agents/` | — | `~/.config/opencode/agents/` |
| **Auto-delegation** | Yes (description) | No (explicit only) | Yes (description) | Yes (description + @) | No (explicit only) | Yes (@mention + Task) |
| **Can write files** | Yes (configurable) | Yes (configurable) | Yes (configurable) | Yes (configurable) | No (read-only) | Yes (configurable) |
| **Model override** | Yes | Yes | Yes | Yes | Yes (v3.67+) | Yes (cross-provider) |
| **Tool restriction** | Yes (whitelist) | Yes (sandbox modes) | Yes (readonly flag) | Yes (tool array) | Fixed (read-only) | Yes (permissions) |
| **Background/async** | No (Agent Teams) | Yes (parallel) | Yes (`is_background`) | No | No | Yes (team messaging) |
| **Inter-agent comms** | Agent Teams (inbox) | No | No | No | No | Peer-to-peer messaging |
| **Nesting** | No | Configurable depth | No | No | No | Configurable |
| **Cross-provider models** | No | No | No | No | Yes (BYOK) | Yes |