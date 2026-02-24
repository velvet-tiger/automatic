# Automatic — Using the MCP Service

Automatic is a desktop application that acts as a **skill registry and MCP configuration hub** for AI agent tooling. It does not run agents itself. Instead, it serves data to agents on request via an MCP server interface.

## When to Use Automatic

Use the Automatic MCP tools when you need to:

- Retrieve an API key for an LLM provider (Anthropic, OpenAI, Gemini, etc.)
- Discover which skills are available in the user's skill registry
- Load a skill's instructions before performing a task
- Find MCP server configurations to suggest or apply
- Sync a project's configurations to its directory

## Available MCP Tools

Automatic exposes the following tools via the `nexus` MCP server (configured as `nexus mcp-serve`):

### `nexus_get_credential`

Retrieve an API key stored in Automatic.

```
provider: string  — e.g. "anthropic", "openai", "gemini"
```

**When to use:** Before making direct API calls that require a key the user has stored in Automatic. Always prefer reading credentials from Automatic rather than asking the user to paste them.

---

### `nexus_list_skills`

List all skill names currently registered in the user's skill registry (`~/.agents/skills/` and `~/.claude/skills/`).

**When to use:** At the start of a session or task to discover what specialised instructions are available. If you find a relevant skill, read it with `nexus_read_skill`.

---

### `nexus_read_skill`

Read the full `SKILL.md` content of a specific skill.

```
name: string  — the skill directory name, e.g. "laravel-specialist"
```

**When to use:** After identifying a relevant skill via `nexus_list_skills`. Load and follow the skill's instructions for the current task.

---

### `nexus_list_mcp_servers`

Return all MCP server configurations stored in the Automatic registry (`~/.nexus/mcp_servers/`).

**When to use:** When the user asks about available MCP servers, or when you need to reference server configs before syncing a project.

---

### `nexus_sync_project`

Sync a project's MCP server configs and skill references to its directory for all configured agent tools (Claude Code, Cursor, OpenCode, etc.).

```
name: string  — the project name as registered in Automatic
```

**When to use:** After the user updates a project's configuration (skills, MCP servers, agents) in Automatic and wants the changes written to the project directory.

## Recommended Workflow

1. **On session start** — call `nexus_list_skills` to see what skills are available. If a skill matches the current task domain, call `nexus_read_skill` to load it.

2. **For credentials** — call `nexus_get_credential` with the provider name instead of asking the user.

3. **For project setup** — call `nexus_list_mcp_servers` to see registered servers, then `nexus_sync_project` to apply the configuration.

## Configuration

Automatic's MCP server is configured in the agent tool's MCP settings:

```json
{
  "mcpServers": {
    "nexus": {
      "command": "nexus",
      "args": ["mcp-serve"]
    }
  }
}
```

The `nexus` binary is the Automatic desktop app binary. When invoked with `mcp-serve`, it starts the MCP server on stdio and does not open any UI.
