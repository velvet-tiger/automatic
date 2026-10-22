---
name: automatic
description: How to use the Automatic MCP service — skill discovery, memory, project config, and session management. Activate at the start of every session in a project managed by Automatic.
authors:
  - Automatic
---

# Automatic — Using the MCP Service

Automatic is a desktop application that acts as a **skill registry, memory store, and MCP configuration hub** for AI agent tooling. It does not run agents itself. Instead, it serves data to agents on request via an MCP server interface.

## When to Use Automatic

Use the Automatic MCP tools when you need to:

- Discover which skills are available in the user's skill registry
- Load a skill's instructions and discover its companion resources
- Search the community skills.sh registry for relevant skills
- Find MCP server configurations to suggest or apply
- Inspect, list, or register the user's projects, and find a project's related projects
- Manage the rules, hooks, and profiles attached to a project
- Check which Claude Code sessions are currently active
- Retrieve a stored API key for an LLM provider
- Sync a project's configurations to its directory
- **Store, retrieve, or search long-term memory across sessions for a specific project**
- Read reference material from the contexts attached to a project

## Available MCP Tools

Automatic exposes the following tools via the `automatic` MCP server (configured as `automatic mcp-serve`):

**The current project.** The server knows which project you are working in. Every tool that takes a `project` (or `name` on `automatic_read_project` and `automatic_sync_project`) uses that project when you omit it. Pass it only to act on another project. It accepts a project's `name`, `local_key` or `id` from `automatic_list_projects`. When a name or id matches several folders, the error lists each one with its `local_key`; pass that key.

### `automatic_list_skills`

List all skill names currently registered in the user's skill registry (`~/.automatic/library/skills/`, with `~/.agents/skills/` and `~/.claude/skills/` scanned read-only).

**When to use:** At the start of a session or task to discover what specialised instructions are available. If you find a relevant skill, read it with `automatic_read_skill`.

---

### `automatic_read_skill`

Read the full `SKILL.md` content of a specific skill. This also automatically discovers and returns a list of any companion resources (scripts, templates, examples, etc.) bundled in the skill directory.

```
name: string      — the skill directory name, e.g. "laravel-specialist"
project?: string  — search this project's own skills first; defaults to the current project
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

Return all MCP server configurations stored in the Automatic registry (`~/.automatic/library/mcp_servers/`).

**When to use:** When the user asks about available MCP servers, or when you need to reference server configs before syncing a project.

---

### `automatic_set_mcp_server_enabled`

Enable or disable one MCP server attached to a project. A disabled server stays attached and is left out of the agent config files. `automatic_read_project` lists it under `disabled_mcp_servers`.

```
project?: string  — defaults to the current project
server: string    — the server name, as listed under `mcp_servers`
enabled: boolean  — true to enable, false to disable
```

The call is refused for a server that is not attached to the project, for the `automatic` server, and for a server an attached profile provides. Asking for the state a server is already in succeeds.

**When to use:** When the user wants a server switched off for a project without detaching it, or switched back on. The call does not sync to disk — call `automatic_sync_project` afterwards.

---

### `automatic_list_projects`

List every project registered in Automatic. Each entry has `name`, `local_key` (one folder on this machine), `id` (the project, shared by every folder of it, such as git worktrees), `directory`, and `current`. `current` marks the project you are working in.

**When to use:** When you need to find out which projects the user has configured, or to act on a project other than the current one.

---

### `automatic_get_related_projects`

Return the peer projects of every Project Group this project belongs to. Each peer has its name, description, directory, and the relative path from this project's directory.

```
project?: string  — defaults to the current project
```

**When to use:** Before searching the filesystem or asking the user for sibling projects. This is the authoritative source. Related projects are not written into the instruction file.

---

### `automatic_read_project`

Read the full configuration for a project: description, directory path, assigned skills, MCP servers, `disabled_mcp_servers` (attached servers that are switched off; absent when none are), providers, configured agent tools, attached `profiles`, `profile_contributions` (which entries each profile provides, including entries the project had before it was attached), `contexts` (attached context slugs), `group_context_contributions` (which of those contexts each project group provides), and `group_profile_contributions` (which of the attached profiles each project group provides).

```
name?: string  — defaults to the current project
```

**When to use:** When you need to understand a project's configured context (e.g. which skills and MCP servers apply, or where the project directory is) before performing work in it.

---

### `automatic_list_profiles`

List every profile in the library. A profile is a live bundle of library references (skills, MCP servers, providers, agents, sub-agents, commands, hooks, rules). Saving a profile keeps every attached project in step.

**When to use:** When the user wants a shared baseline applied to several projects, or asks which profiles exist.

---

### `automatic_read_profile`

Read a profile's full contents.

```
name: string  — the profile name
```

---

### `automatic_attach_profile` / `automatic_detach_profile`

Attach a profile to a project or to a project group, or detach it. Attaching records every entry the profile lists as the profile's contribution: missing entries are added to the project and entries the project already had are adopted. Detaching removes every entry the profile provides, including entries the project had before it was attached.

```
project?: string  — defaults to the current project
group?: string    — a project group name; give `project` or `group`, not both
profile: string   — the profile name
```

With `group`, every member project receives the profile, and the call syncs the members that changed. A profile a group provides cannot be detached from a member project. Detach it from the group.

**When to use:** After the user asks for a project or a group to follow a profile. On a project, neither call syncs to disk — call `automatic_sync_project` afterwards. Do not remove a profile-owned rule or hook with `automatic_detach_rule` / `automatic_detach_hook`; it is re-attached on the next save.

---

### `automatic_list_rules` / `automatic_read_rule`

A rule is a Markdown instruction block that sync writes into a project's instruction files (`CLAUDE.md`, `AGENTS.md` and similar). `automatic_list_rules` lists every rule in the library with its `id` (machine name), `name` (display name) and, for plugin-provided rules, `plugin_id`. `automatic_read_rule` returns one rule's `name`, `content` and optional `plugin_id`.

```
machine_name: string  — read: the rule's machine name
```

**When to use:** To see which rules exist before attaching one, or to read a rule's full text.

---

### `automatic_create_rule` / `automatic_update_rule` / `automatic_delete_rule`

Add, edit or remove a rule in the library.

```
machine_name: string  — lowercase letters, digits and hyphens, starting with a letter
name: string          — create: the display name; update: optional
content: string       — create: the Markdown body; update: optional
```

`automatic_create_rule` fails when the machine name is taken. `automatic_update_rule` needs at least one of `name` or `content`, and refuses plugin-provided rules. `automatic_delete_rule` refuses mandatory rules (such as `automatic-service`) and plugin-provided rules. Deleting a rule does not detach it from projects. They skip it on their next sync.

**When to use:** Only when the user asks for a rule to be added, changed or removed.

---

### `automatic_attach_rule` / `automatic_detach_rule`

Attach a library rule to a project's instruction file, or detach it. Both calls are idempotent.

```
project?: string      — defaults to the current project
machine_name: string  — the rule's machine name
file?: string         — "CLAUDE.md", "AGENTS.md" and so on for one agent's file, or "_project" for every agent file
```

Omit `file` when the project is in unified instruction mode. Per-agent mode requires it. Mandatory rules cannot be detached. A rule an attached profile provides is re-attached on the next save, so detach the profile instead.

**When to use:** When the user wants a rule added to or removed from a project. Neither call syncs to disk — call `automatic_sync_project` afterwards.

---

### `automatic_list_hooks` / `automatic_read_hook`

A hook is a handler that an agent runs on a lifecycle event, such as `SessionStart` or `PreToolUse`. `automatic_list_hooks` lists every hook in the library with its `id`, `name`, `agent`, `event` and optional `plugin_id`. `automatic_read_hook` returns one hook's full definition: `name`, `agent`, `event`, `matcher`, `handler`, `timeout_sec` and optional `plugin_id`.

```
machine_name: string  — read: the hook's machine name
```

---

### `automatic_create_hook` / `automatic_update_hook` / `automatic_delete_hook`

Add, edit or remove a hook in the library.

```
machine_name: string  — the hook's machine name
name: string          — the display name (optional on update)
agent: string         — the target agent id, e.g. "claude" or "codex" (optional on update)
event: string         — the lifecycle event; accepted values depend on the agent (optional on update)
matcher?: string      — e.g. a tool-name regex for PreToolUse / PostToolUse; on update, null clears it
handler: object       — {"kind":"command","command":"..."} or {"kind":"script","interpreter":"bash","script":"..."} (optional on update)
timeout_sec?: number  — on update, null clears it
```

`automatic_create_hook` fails when the machine name is taken. `automatic_update_hook` changes only the fields you pass, and refuses plugin-provided hooks. `automatic_delete_hook` refuses plugin-provided hooks. Deleting a hook does not detach it from projects. They skip it on their next sync.

**When to use:** Only when the user asks for a hook to be added, changed or removed.

---

### `automatic_attach_hook` / `automatic_detach_hook`

Attach a library hook to a project, or detach it. The target agent comes from the hook's library record. Both calls are idempotent.

```
project?: string      — defaults to the current project
machine_name: string  — the hook's machine name
```

A hook an attached profile provides is re-attached on the next save, so detach the profile instead.

**When to use:** When the user wants a hook added to or removed from a project. Neither call syncs to disk — call `automatic_sync_project` afterwards.

---

### `automatic_list_contexts`

List the contexts attached to a project. A context is a named collection of reference material that agents read on demand: documentation pages, local files, URLs, or sources in the Automatic cloud. Nothing from a context is written into the project. Each entry carries `group` when a project group provides it.

```
project?: string  — defaults to the current project
all?: boolean     — list every context in the library instead
```

With no current project and no `project`, every context in the library is listed.

**When to use:** At the start of work in a project, to find the reference material the user attached to it.

---

### `automatic_read_context`

Read a context's description and its `sources` (each with an `id` and a `kind`).

```
context: string  — the context slug
```

---

### `automatic_list_context_entries` / `automatic_read_context_entry`

List the readable entries of one source, then read one entry's text. A URL source has a single entry named `content`.

```
context: string  — the context slug
source: string   — the source id from automatic_read_context
path: string     — the entry path (read only)
```

**When to use:** When the task needs the material a context points at. Read only the entries that are relevant. Cloud contexts and cloud sources need the user to be signed in to Automatic.

---

### `automatic_attach_context` / `automatic_detach_context`

Attach a context to a project or a project group, or detach it. Give `project` or `group`, not both. Give neither to use the current project. A group's contexts reach every member project. A context a group provides cannot be detached from a member project; detach it from the group.

```
context: string   — the context slug
project?: string  — defaults to the current project
group?: string    — the project group name
```

**When to use:** Only when the user asks. No sync is needed.

---

### `automatic_create_context` / `automatic_write_context_page`

Create a local context, then write Markdown pages into it. Address a page by `title` and optional `folder` (the same title in the same folder replaces that page), or replace an existing page by its `path`.

```
name: string          — create: the context name
description?: string  — create: what the context holds
context: string       — write: the context slug
title?: string        — write: the page title
folder?: string       — write: e.g. "Guides" or "Guides/Troubleshooting"
path?: string         — write: an existing page path, instead of title
content: string       — write: the page body in Markdown
```

**When to use:** When the user asks, or to record something durable the user has agreed, such as a decision. Read the existing pages first and update one rather than adding a near-duplicate. `automatic_move_context_page` and `automatic_delete_context_page` move and delete pages.

---

### `automatic_add_context_folder` / `automatic_add_context_web_page` / `automatic_remove_context_source`

Link a local folder or file, or a web page, into a local context. Agents then read it where it is; nothing is copied. Remove a linked source by its id. The context's pages cannot be removed this way.

```
context: string       — the context slug
path: string          — folder: an absolute path to a folder or file
url: string           — web page: an http or https address
name: string          — folder / web page: a short name
description?: string  — folder / web page: when an agent should look here
ttl_secs?: number     — web page: seconds a downloaded copy is reused (default 3600, 0 = every read)
source: string        — remove: the source id from automatic_read_context
```

**When to use:** Only when the user asks. The user's settings decide which folders you may link: by default only folders inside registered projects, and possibly none. Hidden, credential and system folders are always refused, and only Markdown and text files are shared. Web pages you add may only reach public addresses until the user keeps them in the app. Everything you link is marked as added by an agent. Cloud sources can only be added by the user in the Automatic app.

---

### `automatic_register_project`

Register a new project in Automatic.

```
name: string               — unique name for the new project
directory: string          — absolute path to the project's working directory (must already exist on disk)
description: string        — optional short description
agents: string[]           — optional agent tool ids, e.g. ["claude", "cursor", "codex"]
```

**When to use:** When the user wants to bring a new project under Automatic management. The call fails when the name is already taken, the directory is already registered to another project, or the directory holds an unregistered Automatic config — in the last case ask the user to import it from the Automatic app instead. When `agents` is provided, agent configuration files are synced into the directory immediately.

---

### `automatic_sync_project`

Sync a project's MCP server configs and skill references to its directory for all configured agent tools (Claude Code, Cursor, OpenCode, etc.).

```
name?: string  — defaults to the current project
```

**When to use:** After the user updates a project's configuration (skills, MCP servers, agents) in Automatic and wants the changes written to the project directory.

---

### `automatic_list_sessions`

List active Claude Code sessions tracked by Automatic's hooks. Each entry includes session id, working directory (`cwd`), model, and `started_at` timestamp.

**When to use:** When you want to know what other Claude Code sessions are currently active — useful for awareness of parallel work or cross-session context.

---

### `automatic_get_credential`

Return the API key stored in Automatic for an LLM provider. Only recognised provider ids are accepted.

```
provider: string  — e.g. "anthropic", "openai", "gemini"
```

**When to use:** When a task needs a provider API key that the user keeps in Automatic. Never print the key or write it into a file.

---

### Agent Memory Tools

Automatic provides a persistent key-value store for agents to retain context, user preferences, and learnings over time on a per-project basis.

Every memory tool takes an optional `project`, which defaults to the current project. All folders of one project share its memory.

- **`automatic_store_memory`**: Stores a memory entry. Takes `key`, `value`, and optional `source`.
- **`automatic_get_memory`**: Retrieves a specific memory entry by its `key`.
- **`automatic_list_memories`**: Lists all stored memory keys, optionally filtered by a `pattern`.
- **`automatic_search_memories`**: Searches both keys and values for a `query` string.
- **`automatic_delete_memory`**: Deletes a specific memory entry by `key`.
- **`automatic_clear_memories`**: Clears all memories (requires `confirm: true` and optional `pattern`).
- **`automatic_read_claude_memory`**: Reads Claude Code's own auto-memory files for the project (`MEMORY.md` and any topic files under `~/.claude/projects/<encoded-path>/memory/`). Use it to see what Claude has learned, then promote durable entries with `automatic_store_memory`.

**When to use:** Proactively store memory when you learn a significant project-specific rule, a user preference, or architectural decision that you (or other agents) will need in future sessions. Search memories at the start of complex tasks to see if previous guidance applies.

---

### Agent Feature Tools

The `automatic_*_feature` tools track work items on a project's Build board. They work only on projects that have the Build tool. On other projects they return an error saying Build is not enabled. Projects with Build carry the `automatic-features` skill and the "Build: Feature Tracking" rule, which describe the workflow.

---

## Recommended Workflow

1. **On session start** — make these four calls:
   - `automatic_list_skills` to see what skills are available. If a skill matches the current task domain, call `automatic_read_skill` to load it and view its companion resources.
   - `automatic_search_memories` with relevant keywords to retrieve past learnings for the current project.
   - `automatic_read_project` to load the current project's configured skills, MCP servers, agents, and directory. To work on another project, find it with `automatic_list_projects` first.
   - `automatic_list_contexts` to see the reference material attached to the project.

2. **Before project-specific decisions** — consult the relevant contexts before deciding on conventions, architecture, product behaviour, or wording. For sibling projects, call `automatic_get_related_projects` before searching the filesystem.

3. **For rules, hooks, and profiles** — attach or detach them only when the user asks, then call `automatic_sync_project`. Edit or detach a profile instead of removing an entry the profile provides.

4. **For project setup** — call `automatic_list_mcp_servers` to see registered servers, then `automatic_sync_project` to apply the configuration.

5. **For skill discovery** — call `automatic_search_skills` to find community skills relevant to the task at hand.

6. **Wrapping up a session** — Call `automatic_store_memory` to capture any new project-specific conventions, pitfalls, or setup steps discovered so they aren't lost in future sessions.

7. **For project features** — only when the project has the Build tool, follow the `automatic-features` skill.

## Configuration

Automatic's MCP server is configured in the agent tool's MCP settings:

```json
{
  "mcpServers": {
    "automatic": {
      "command": "automatic",
      "args": ["mcp-serve"],
      "env": {
        "AUTOMATIC_PROJECT": "my-project"
      }
    }
  }
}
```

The `automatic` binary is the Automatic desktop app binary. When invoked with `mcp-serve`, it starts the MCP server on stdio and does not open any UI. Project sync writes this entry for you, with the binary's full path as `command`. `AUTOMATIC_PROJECT` tells the server which project it is working in. Without it, the server matches the working directory against registered projects.
