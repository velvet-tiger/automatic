# Working with the Automatic MCP Service

This project is managed by Automatic, a desktop hub that provides skills, rules, hooks, memory, feature tracking, and MCP server configs to agents via an MCP interface. The Automatic MCP server is always available in this project.

## Session Start

1. Call `automatic_list_skills` to discover available skills. If any match the current task domain, call `automatic_read_skill` to load instructions and companion resources.
2. Call `automatic_search_memories` with relevant keywords for this project to retrieve past learnings, conventions, and decisions.
3. Call `automatic_read_project` with this project's name to understand the configured skills, MCP servers, agents, and directory.

## During Work

- **Skills** — Follow loaded skill instructions. Skills may include companion scripts, templates, or reference docs in their directory.
- **MCP Servers** — Call `automatic_list_mcp_servers` to see what servers are registered. Call `automatic_sync_project` after configuration changes.
- **Skill Discovery** — Call `automatic_search_skills` to find community skills on skills.sh when you need specialised guidance not covered by installed skills.
- **Related Projects** — Before searching the filesystem or asking the user for sibling projects, call `automatic_get_related_projects` with this project's name. It returns peer projects (name, description, directory, and the relative path from this project) for every Project Group this project belongs to. This is the authoritative source — related projects are intentionally not written into the instruction file.
- **Other Projects** — Call `automatic_list_projects` to see every project name registered in Automatic.
- **Project Context** — Call `automatic_get_project_context` for a project's commands, entry points, architecture concepts, conventions, gotchas, a merged documentation index, and the rules currently attached to each instruction file.

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
