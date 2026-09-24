# Working with the Automatic MCP Service

This project is managed by Automatic, a desktop hub that provides skills, rules, hooks, memory, feature tracking, and MCP server configs to agents via an MCP interface. The Automatic MCP server is always available in this project.

## Session Start

1. Call `automatic_list_skills` to discover available skills. If any match the current task domain, call `automatic_read_skill` to load instructions and companion resources.
2. Call `automatic_search_memories` with relevant keywords for this project to retrieve past learnings, conventions, and decisions.
3. Call `automatic_read_project` with this project's name to understand the configured skills, MCP servers, agents, and directory.
4. Call `automatic_list_contexts` with this project's name. Each context is reference material the user attached for this project, such as coding standards, product docs, or decisions. Read each description, and keep the list in mind for the rest of the session.

## During Work

- **Skills** — Follow loaded skill instructions. Skills may include companion scripts, templates, or reference docs in their directory.
- **MCP Servers** — Call `automatic_list_mcp_servers` to see what servers are registered. Call `automatic_sync_project` after configuration changes.
- **Skill Discovery** — Call `automatic_search_skills` to find community skills on skills.sh when you need specialised guidance not covered by installed skills.
- **Related Projects** — Before searching the filesystem or asking the user for sibling projects, call `automatic_get_related_projects` with this project's name. It returns peer projects (name, description, directory, and the relative path from this project) for every Project Group this project belongs to. This is the authoritative source — related projects are intentionally not written into the instruction file.
- **Other Projects** — Call `automatic_list_projects` to see every project name registered in Automatic.
- **Registering Projects** — Call `automatic_register_project` with a unique name and an absolute directory path to bring a new project under Automatic management. Optionally pass agent ids (e.g. `claude`) to sync their config files immediately. The call is refused when the directory already belongs to a registered project or holds an unregistered Automatic config — ask the user how to proceed in those cases.

## Rules

Rules are markdown instruction blocks attached to a project's instruction files (this file is one of them):

- `automatic_list_rules` — list every rule in the library (machine name, display name, plugin owner if any).
- `automatic_read_rule` — read a rule's full content by machine name.
- `automatic_create_rule` / `automatic_update_rule` — add a new rule or edit an existing one's name and/or content. `automatic_update_rule` refuses plugin-provided rules.
- `automatic_attach_rule` / `automatic_detach_rule` — wire a rule into a project's instruction file. Neither call syncs to disk on its own — call `automatic_sync_project` afterwards.
- `automatic_delete_rule` — remove a rule from the library. Mandatory rules (including this one) and plugin-provided rules cannot be deleted. Deleting a rule does not detach it from projects that reference it; they silently skip it on next sync.

## Hooks

Hooks are event-triggered handlers (e.g. on session start, before a tool call) scoped to a specific agent and event:

- `automatic_list_hooks` — list every hook in the library (machine name, name, agent, event, plugin owner if any).
- `automatic_read_hook` — read a hook's full definition (name, agent, event, matcher, handler, timeout).
- `automatic_create_hook` / `automatic_update_hook` — add a new hook or edit an existing one.
- `automatic_delete_hook` — remove a hook from the library. Plugin-provided hooks cannot be deleted. Projects referencing a deleted hook silently skip it on next sync.
- `automatic_attach_hook` / `automatic_detach_hook` — wire a hook into a project (the target agent is inferred from the hook's library record). Neither call syncs to disk on its own — call `automatic_sync_project` afterwards.

## Profiles

Profiles are live bundles of library references (skills, MCP servers, providers, agents, sub-agents, commands, hooks, rules) shared across projects. Saving a profile brings every attached project back in step. A profile owns every entry it lists on an attached project, including entries the project had before the profile was attached. Entries no attached profile lists stay the project's own.

- `automatic_list_profiles` — list every profile in the library (name, description).
- `automatic_read_profile` — read a profile's full contents by name.
- `automatic_attach_profile` / `automatic_detach_profile` — attach or detach a profile. Detaching removes every entry the profile provides, including entries the project had before it was attached. Neither call syncs to disk on its own — call `automatic_sync_project` afterwards.
- `automatic_read_project` reports `profiles` and `profile_contributions`. An entry listed under `profile_contributions` belongs to that profile: detaching it with `automatic_detach_rule` or `automatic_detach_hook` is undone on the project's next save. Edit or detach the profile instead.

## Contexts

Contexts hold what the user wants agents to know about this project. Before you decide on conventions, architecture, product behaviour, or wording, check whether an attached context covers it. The context wins over your assumptions and over general best practice.

- `automatic_list_contexts` — pass `project` to list the contexts attached to this project. Each carries `group` when a project group provides it.
- Read on demand: `automatic_read_context` shows a context's sources. `automatic_list_context_entries` lists a source's entries. Pages can sit in folders, so paths look like `guides/setup.md`. `automatic_read_context_entry` reads one entry. Read only what the task needs.
- If a context and the code disagree, say so to the user. Don't silently pick one.
- If a read fails (for example, a cloud context when the user is signed out), tell the user and carry on without it.
- `automatic_attach_context` / `automatic_detach_context` — attach or detach contexts only when the user asks. A context a group provides must be detached from the group.
- Writing: `automatic_create_context` creates a local context. `automatic_write_context_page` creates or replaces a page by `title` and optional `folder`, or replaces one by `path`. `automatic_move_context_page` and `automatic_delete_context_page` move and delete pages. Write only when the user asks, or to record something durable the user has agreed, such as a decision. Search the existing pages first and update one rather than adding a near-duplicate.
- Linking: `automatic_add_context_folder` links a local folder or file, and `automatic_add_context_web_page` links a web page. `automatic_remove_context_source` removes a linked source. Link or remove only when the user asks. The user's settings decide which folders you may link. By default that is folders inside registered projects. Hidden, credential and system folders are always refused. Web pages you add may only reach public addresses until the user keeps them. Only the user can add cloud sources, in the Automatic app.

## Memory

Use the memory tools to persist and retrieve project-specific context across sessions:

- `automatic_store_memory` — store a key-value entry. Set the `source` parameter so the origin is traceable. Use descriptive, hierarchical keys (e.g. `conventions/naming`, `setup/database`, `decisions/auth-approach`).
- `automatic_get_memory` — retrieve a specific entry by key.
- `automatic_list_memories` — list every stored entry, optionally filtered by a key pattern.
- `automatic_search_memories` — case-insensitive substring search across keys and values. Search before making assumptions; previous sessions may have captured relevant context.
- `automatic_delete_memory` — remove a single entry by key.
- `automatic_clear_memories` — remove all entries for a project, optionally filtered by pattern. Requires explicit confirmation and cannot be undone; use with caution.
- `automatic_read_claude_memory` — read Claude Code's own auto-memory files for this project (`MEMORY.md` and any topic files under `~/.claude/projects/<encoded-path>/memory/`). Use this to see what Claude has already learned, then call `automatic_store_memory` to promote anything durable into Automatic's structured store.

## Features

Automatic provides project-scoped feature tracking for managing work items across sessions:

- Call `automatic_list_features` to see planned work. Filter by state (`backlog`, `todo`, `in_progress`, `review`, `complete`, `cancelled`). Pass `include_archived: true` to list archived features instead.
- Before starting a task, call `automatic_set_feature_state` to move it to `in_progress`.
- During work, call `automatic_add_feature_update` to log significant progress, decisions, or blockers. Updates are append-only and ordered newest-first.
- On completion, move the feature to `review` so the user can verify before marking `complete`.
- If new work is discovered, call `automatic_create_feature` to capture it in the backlog.
- Use `automatic_get_feature` for full detail on one feature, `automatic_update_feature` to edit its metadata (title, description, priority, assignee, tags, linked files, effort), and `automatic_archive_feature` / `automatic_unarchive_feature` to hide or restore one without losing its state. `automatic_delete_feature` permanently removes a feature and all its updates; this cannot be undone.

## Credentials

Call `automatic_get_credential` to retrieve a stored API key for a known LLM provider (e.g. `anthropic`, `openai`). Only recognised provider ids are accepted.

## Sessions

Call `automatic_list_sessions` to see active Claude Code sessions tracked by Automatic's hooks (session id, working directory, model, started_at).

## Session End

Before finishing a session, call `automatic_store_memory` to capture any new project-specific rules, pitfalls, setup steps, or decisions discovered during the session. This prevents knowledge loss across sessions.
