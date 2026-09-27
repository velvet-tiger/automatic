# Build: Feature Tracking

This project uses Automatic's Build tool. Build tracks the project's work items as features, and the user sees them on the project's Build board. Use the feature tools below to keep that board current.

- Call `automatic_list_features` to see planned work. Filter by state (`backlog`, `todo`, `in_progress`, `review`, `complete`, `cancelled`). Pass `include_archived: true` to list archived features instead.
- Before starting a task, call `automatic_set_feature_state` to move it to `in_progress`.
- During work, call `automatic_add_feature_update` to log significant progress, decisions, or blockers. Updates are append-only and ordered newest-first.
- On completion, move the feature to `review` so the user can verify before marking `complete`.
- If new work is discovered, call `automatic_create_feature` to capture it in the backlog.
- Use `automatic_get_feature` for full detail on one feature, `automatic_update_feature` to edit its metadata (title, description, priority, assignee, tags, linked files, effort), and `automatic_archive_feature` / `automatic_unarchive_feature` to hide or restore one without losing its state. `automatic_delete_feature` permanently removes a feature and all its updates; this cannot be undone.

For the full workflow, load the `automatic-features` skill.
