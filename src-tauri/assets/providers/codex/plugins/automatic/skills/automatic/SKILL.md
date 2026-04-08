---
name: automatic
description: Use Automatic's MCP tools for project context, skills, memory, and feature tracking.
authors:
  - Automatic
---

# Automatic

This plugin exposes the Automatic desktop app as an MCP server named `automatic`.

Use Automatic when you need to:

- list or read registered Automatic projects
- pull structured project context before changing code
- inspect installed skills or search community skills
- store and retrieve project memory across sessions
- track planned work through Automatic features

## Recommended workflow

1. Call `automatic_list_projects` and `automatic_read_project` to confirm the project you are working in.
2. Call `automatic_get_project_context` for commands, entry points, conventions, gotchas, and docs.
3. Call `automatic_list_skills` and `automatic_read_skill` when the task may benefit from a project or domain skill.
4. Call `automatic_search_memories` before making assumptions and `automatic_store_memory` before wrapping up meaningful discoveries.
5. Call `automatic_list_features` when work should be aligned with planned tasks. Move features through `automatic_set_feature_state` and log progress with `automatic_add_feature_update`.

## Core tool groups

### Project and skills

- `automatic_list_projects`
- `automatic_read_project`
- `automatic_get_project_context`
- `automatic_list_skills`
- `automatic_read_skill`
- `automatic_search_skills`
- `automatic_sync_project`

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
