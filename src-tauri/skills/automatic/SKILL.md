# Automatic — Using the MCP Service

Automatic is a desktop application that acts as a **skill registry, memory store, and MCP configuration hub** for AI agent tooling. It does not run agents itself. Instead, it serves data to agents on request via an MCP server interface.

## When to Use Automatic

Use the Automatic MCP tools when you need to:

- Discover which skills are available in the user's skill registry
- Load a skill's instructions and discover its companion resources
- Search the community skills.sh registry for relevant skills
- Find MCP server configurations to suggest or apply
- Inspect or list the user's registered projects
- Check which Claude Code sessions are currently active
- Sync a project's configurations to its directory
- **Store, retrieve, or search long-term memory across sessions for a specific project**

## Available MCP Tools

Automatic exposes the following tools via the `nexus` MCP server (configured as `nexus mcp-serve`):

### `automatic_list_skills`

List all skill names currently registered in the user's skill registry (`~/.agents/skills/` and `~/.claude/skills/`).

**When to use:** At the start of a session or task to discover what specialised instructions are available. If you find a relevant skill, read it with `automatic_read_skill`.

---

### `automatic_read_skill`

Read the full `SKILL.md` content of a specific skill. This also automatically discovers and returns a list of any companion resources (scripts, templates, examples, etc.) bundled in the skill directory.

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

Return all MCP server configurations stored in the Automatic registry (`~/.automatic/mcp_servers/`).

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

### `automatic_sync_project`

Sync a project's MCP server configs and skill references to its directory for all configured agent tools (Claude Code, Cursor, OpenCode, etc.).

```
name: string  — the project name as registered in Automatic
```

**When to use:** After the user updates a project's configuration (skills, MCP servers, agents) in Automatic and wants the changes written to the project directory.

---

### `automatic_list_sessions`

List active Claude Code sessions tracked by the Nexus hooks. Each entry includes session id, working directory (`cwd`), model, and `started_at` timestamp.

**When to use:** When you want to know what other Claude Code sessions are currently active — useful for awareness of parallel work or cross-session context.

---

### Agent Memory Tools

Automatic provides a persistent key-value store for agents to retain context, user preferences, and learnings over time on a per-project basis.

- **`automatic_store_memory`**: Stores a memory entry. Takes `project`, `key`, `value`, and optional `source`.
- **`automatic_get_memory`**: Retrieves a specific memory entry by its `project` and `key`.
- **`automatic_list_memories`**: Lists all stored memory keys for a `project`, optionally filtered by a `pattern`.
- **`automatic_search_memories`**: Searches both keys and values for a `query` string within a `project`.
- **`automatic_delete_memory`**: Deletes a specific memory entry by `project` and `key`.
- **`automatic_clear_memories`**: Clears all memories for a `project` (requires `confirm: true` and optional `pattern`).

**When to use:** Proactively store memory when you learn a significant project-specific rule, a user preference, or architectural decision that you (or other agents) will need in future sessions. Search memories at the start of complex tasks to see if previous guidance applies.

---

## Recommended Workflow

1. **On session start** — call `automatic_list_skills` to see what skills are available. If a skill matches the current task domain, call `automatic_read_skill` to load it and view its companion resources. Optionally call `automatic_search_memories` to retrieve past learnings for the current project.

2. **For project context** — call `automatic_list_projects` to find the relevant project, then `automatic_read_project` to load its full configuration.

3. **For project setup** — call `automatic_list_mcp_servers` to see registered servers, then `automatic_sync_project` to apply the configuration.

4. **For skill discovery** — call `automatic_search_skills` to find community skills relevant to the task at hand.

5. **Wrapping up a session** — Call `automatic_store_memory` to capture any new project-specific conventions, pitfalls, or setup steps discovered so they aren't lost in future sessions.

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
