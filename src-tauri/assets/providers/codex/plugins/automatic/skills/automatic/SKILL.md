---
name: automatic
description: Use Automatic's MCP tools for projects, skills, memory, and feature tracking.
authors:
  - Automatic
---

# Automatic

This plugin exposes the Automatic desktop app as an MCP server named `automatic`.

Use Automatic when you need to:

- list or read registered Automatic projects
- inspect installed skills or search community skills
- store and retrieve project memory across sessions
- track planned work through Automatic features
- read reference material from the contexts attached to a project

## Recommended workflow

1. Call `automatic_list_projects` and `automatic_read_project` to confirm the project you are working in.
2. Call `automatic_list_contexts`, and consult relevant contexts before making project-specific decisions about conventions, architecture, product behaviour, or wording.
3. Call `automatic_list_skills` and `automatic_read_skill` when the task may benefit from a project or domain skill.
4. Call `automatic_search_memories` before making assumptions and `automatic_store_memory` before wrapping up meaningful discoveries.
5. Call `automatic_list_features` when work should be aligned with planned tasks. Move features through `automatic_set_feature_state` and log progress with `automatic_add_feature_update`.

## Core tool groups

### Project and skills

- `automatic_list_projects`
- `automatic_read_project`
- `automatic_list_skills`
- `automatic_read_skill`
- `automatic_search_skills`
- `automatic_sync_project`

### Contexts

- `automatic_list_contexts` (the project the server runs in, or, if it is inside none, the one project every client root resolves to; committed slugs stay pending until the user attaches them)
- `automatic_read_context`
- `automatic_list_context_entries`
- `automatic_read_context_entry`
- `automatic_attach_context` / `automatic_detach_context` (only when the user asks)
- `automatic_create_context`, `automatic_write_context_page`, `automatic_move_context_page`, `automatic_delete_context_page` (only when the user asks, or to record an agreed decision)
- `automatic_add_context_folder`, `automatic_add_context_web_page`, `automatic_remove_context_source` (only when the user asks; a folder must be inside this project's own folder, and the user's settings can narrow that further)

### Memory

- `automatic_store_memory`
- `automatic_get_memory`
- `automatic_list_memories`
- `automatic_search_memories`
- `automatic_delete_memory`
- `automatic_clear_memories`

### Features

- `automatic_list_features`
- `automatic_get_feature`
- `automatic_create_feature`
- `automatic_update_feature`
- `automatic_set_feature_state`
- `automatic_add_feature_update`
- `automatic_archive_feature`
- `automatic_unarchive_feature`
- `automatic_delete_feature`

### Sessions and credentials

- `automatic_list_sessions`
- `automatic_get_credential`
- `automatic_list_mcp_servers`
- `automatic_get_related_projects`
- `automatic_register_project` refuses a folder that holds the one the server runs in, or a client root; a project it registers reads nothing until the user keeps it in the app
- `automatic_read_claude_memory`

## MCP configuration

This plugin registers Automatic with:

```json
{
  "mcpServers": {
    "automatic": {
      "command": "automatic",
      "args": ["mcp-serve"]
    }
  }
}
```

The `automatic` binary is the Automatic desktop app. When invoked with `mcp-serve`, it starts the MCP server over stdio instead of opening the desktop UI.
