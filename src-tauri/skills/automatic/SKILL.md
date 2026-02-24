# Automatic — Using the MCP Service

Automatic is a desktop application that acts as a **skill registry and MCP configuration hub** for AI agent tooling. It does not run agents itself. Instead, it serves data to agents on request via an MCP server interface.

## When to Use Automatic

Use the Automatic MCP tools when you need to:

- Retrieve an API key for an LLM provider (Anthropic, OpenAI, Gemini, etc.)
- Discover which skills are available in the user's skill registry
- Load a skill's instructions before performing a task
- Search the community skills.sh registry for relevant skills
- Find MCP server configurations to suggest or apply
- Inspect or list the user's registered projects
- Check which Claude Code sessions are currently active
- Sync a project's configurations to its directory

## Available MCP Tools

Automatic exposes the following tools via the `nexus` MCP server (configured as `nexus mcp-serve`):

### `automatic_get_credential`

Retrieve an API key stored in Automatic.

```
provider: string  — e.g. "anthropic", "openai", "gemini"
```

**When to use:** Before making direct API calls that require a key the user has stored in Automatic. Always prefer reading credentials from Automatic rather than asking the user to paste them.

---

### `automatic_list_skills`

List all skill names currently registered in the user's skill registry (`~/.agents/skills/` and `~/.claude/skills/`).

**When to use:** At the start of a session or task to discover what specialised instructions are available. If you find a relevant skill, read it with `automatic_read_skill`.

---

### `automatic_read_skill`

Read the full `SKILL.md` content of a specific skill.

```
name: string  — the skill directory name, e.g. "laravel-specialist"
```

**When to use:** After identifying a relevant skill via `automatic_list_skills`. Load and follow the skill's instructions for the current task.

---

### `automatic_search_skills`

Search the [skills.sh](https://skills.sh) community registry for skills matching a query. Returns skill names, install counts, and source repos.

```
query: string  — skill name, topic, or keyword, e.g. "react", "laravel", "docker"
```

**When to use:** When you or the user want to discover community-published skills that are not yet installed locally. Follow up by fetching the skill content and suggesting installation via Automatic.

---

### `automatic_list_mcp_servers`

Return all MCP server configurations stored in the Automatic registry (`~/.nexus/mcp_servers/`).

**When to use:** When the user asks about available MCP servers, or when you need to reference server configs before syncing a project.

---

### `automatic_list_projects`

List all project names registered in Automatic.

**When to use:** When you need to find out which projects the user has configured, before reading a specific project or syncing it.

---

### `automatic_read_project`

Read the full configuration for a named project: description, directory path, assigned skills, MCP servers, providers, and configured agent tools.

```
name: string  — the project name as registered in Automatic
```

**When to use:** When you need to understand a project's configured context (e.g. which skills and MCP servers apply, or where the project directory is) before performing work in it.

---

### `automatic_list_sessions`

List active Claude Code sessions tracked by the Nexus hooks. Each entry includes session id, working directory (`cwd`), model, and `started_at` timestamp.

**When to use:** When you want to know what other Claude Code sessions are currently active — useful for awareness of parallel work or cross-session context.

---

### `automatic_sync_project`

Sync a project's MCP server configs and skill references to its directory for all configured agent tools (Claude Code, Cursor, OpenCode, etc.).

```
name: string  — the project name as registered in Automatic
```

**When to use:** After the user updates a project's configuration (skills, MCP servers, agents) in Automatic and wants the changes written to the project directory.

---

## Recommended Workflow

1. **On session start** — call `automatic_list_skills` to see what skills are available. If a skill matches the current task domain, call `automatic_read_skill` to load it.

2. **For credentials** — call `automatic_get_credential` with the provider name instead of asking the user.

3. **For project context** — call `automatic_list_projects` to find the relevant project, then `automatic_read_project` to load its full configuration.

4. **For project setup** — call `automatic_list_mcp_servers` to see registered servers, then `automatic_sync_project` to apply the configuration.

5. **For skill discovery** — call `automatic_search_skills` to find community skills relevant to the task at hand.

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
