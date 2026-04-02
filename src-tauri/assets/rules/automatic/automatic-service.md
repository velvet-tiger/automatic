# Working with the Automatic MCP Service

This project is managed by Automatic, a desktop hub that provides skills, memory, and MCP server configs to agents via an MCP interface. The Automatic MCP server is always available in this project.

## Session Start

1. Call `automatic_list_skills` to discover available skills. If any match the current task domain, call `automatic_read_skill` to load instructions and companion resources.
2. Call `automatic_search_memories` with relevant keywords for this project to retrieve past learnings, conventions, and decisions.
3. Call `automatic_read_project` with this project's name to understand the configured skills, MCP servers, agents, and directory.

## During Work

- **Skills** — Follow loaded skill instructions. Skills may include companion scripts, templates, or reference docs in their directory.
- **MCP Servers** — Call `automatic_list_mcp_servers` to see what servers are registered. Call `automatic_sync_project` after configuration changes.
- **Skill Discovery** — Call `automatic_search_skills` to find community skills on skills.sh when you need specialised guidance not covered by installed skills.

## Memory

Use the memory tools to persist and retrieve project-specific context across sessions:

- **Store** meaningful learnings: architectural decisions, resolved gotchas, user preferences, environment quirks, naming conventions.
- **Search** before making assumptions — previous sessions may have captured relevant context.
- **Key format** — Use descriptive, hierarchical keys (e.g. `conventions/naming`, `setup/database`, `decisions/auth-approach`).
- **Source** — Set the `source` parameter when storing memory so the origin is traceable.

## Features

Automatic provides project-scoped feature tracking for managing work items across sessions:

- Call `automatic_list_features` to see planned work. Filter by state (`backlog`, `todo`, `in_progress`, `review`, `complete`, `cancelled`).
- Before starting a task, call `automatic_set_feature_state` to move it to `in_progress`.
- During work, call `automatic_add_feature_update` to log significant progress, decisions, or blockers.
- On completion, move the feature to `review` so the user can verify before marking `complete`.
- If new work is discovered, call `automatic_create_feature` to capture it in the backlog.

## Session End

Before finishing a session, call `automatic_store_memory` to capture any new project-specific rules, pitfalls, setup steps, or decisions discovered during the session. This prevents knowledge loss across sessions.
