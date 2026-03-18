---
name: automatic-features
description: How to use Automatic's feature tracking system — creating, progressing, and updating per-project work items via the Automatic MCP service. Activate when working on a project managed by Automatic that has feature tracking enabled.
authors:
  - Automatic
---

# Automatic Features — Agent Guide

Automatic provides per-project **feature tracking**: a structured backlog of work items visible to the user in the Automatic UI and readable/writable by agents via MCP tools.

Features are the shared source of truth for what needs to be built. Users create and prioritise features; agents pick them up, progress them, and log updates as work proceeds.

## Feature Model

Each feature has:

| Field | Values |
|---|---|
| `id` | UUID (returned on create — store it for subsequent calls) |
| `title` | Short description of the work |
| `description` | Full markdown specification |
| `state` | `backlog` · `todo` · `in_progress` · `review` · `complete` · `cancelled` |
| `priority` | `low` · `medium` · `high` |
| `effort` | `xs` · `s` · `m` · `l` · `xl` |
| `assignee` | Agent id or name (free text) |
| `tags` | List of strings for filtering |
| `linked_files` | File paths in the project this feature relates to |
| `archived` | `true` if archived — hidden from Kanban board; retrieve with `include_archived: true` |
| `updates` | Append-only log of markdown progress notes |

> **Archived ≠ Cancelled.** Archiving is a separate flag that hides a feature from normal views without changing its lifecycle state. An archived feature retains its `state` value and is restored to its original column when unarchived. Use `cancelled` when work is deliberately abandoned; use archive for features that are deferred or parked without being deleted.

## MCP Tools

All tools require a `project` parameter matching the project name registered in Automatic. Use `automatic_list_projects` if you are unsure of the correct name.

---

### `automatic_list_features`

List all features for a project, grouped by state. **Active and archived sets are mutually exclusive** — a single call returns one or the other, not both.

```
project:          string          — project name
state:            string (opt)    — filter to one state: backlog | todo | in_progress | review | complete | cancelled
include_archived: boolean (opt)   — default false; pass true to list archived features instead of active ones
```

Returns titles, IDs, priorities, effort, and assignees. **Always call this at session start** with `state: "todo"` to see what is planned for you. Archived features are excluded automatically.

---

### `automatic_get_feature`

Get full detail for a single feature, including its description and complete update history. Works for both active and archived features.

```
project:    string  — project name
feature_id: string  — UUID from list_features
```

The response includes an `Archived` field so you know immediately whether the feature is in its normal state or archived. Read this before starting work so you understand the full specification and any prior progress notes.

---

### `automatic_create_feature`

Create a new feature in the project backlog.

```
project:      string         — project name (required)
title:        string         — short title (required)
description:  string (opt)   — markdown specification
priority:     string (opt)   — low | medium | high (default: medium)
assignee:     string (opt)   — agent id or name
tags:         string[] (opt) — searchable labels
linked_files: string[] (opt) — relevant file paths
effort:       string (opt)   — xs | s | m | l | xl
created_by:   string (opt)   — identifies which agent created it
```

Returns the created feature including its `id`. **Save the id** — you will need it for all subsequent calls.

**When to use:** When you discover work during implementation that is not yet tracked. Capture it rather than doing it silently.

---

### `automatic_update_feature`

Update a feature's metadata fields. Omit any field to leave it unchanged.

```
project:      string         — project name
feature_id:   string         — UUID
title:        string (opt)
description:  string (opt)
priority:     string (opt)
assignee:     string (opt)
tags:         string[] (opt)
linked_files: string[] (opt)
effort:       string (opt)
```

**When to use:** To refine the description after investigating the codebase, or to correct priority/effort estimates.

---

### `automatic_set_feature_state`

Transition a feature to a new lifecycle state.

```
project:    string  — project name
feature_id: string  — UUID
state:      string  — backlog | todo | in_progress | review | complete | cancelled
```

**When to use:** Call this at the key transition points in your workflow (see below). Do not skip states — move through them in order so the user can follow progress in the UI.

---

### `automatic_archive_feature`

Archive a feature, hiding it from the Kanban board and from default `list_features` results. The feature's `state` is preserved unchanged so it can be restored to its original column when unarchived.

```
project:    string  — project name
feature_id: string  — UUID
```

**When to use:** When a feature is being deferred indefinitely, is no longer relevant, or is parked at the user's request — but should not be permanently deleted. Do **not** archive features as part of normal workflow progression; use `set_feature_state` for that.

---

### `automatic_unarchive_feature`

Restore an archived feature to active status. It reappears on the Kanban board in its original state column.

```
project:    string  — project name
feature_id: string  — UUID of an archived feature
```

**When to use:** When the user decides to resume work on a previously parked feature.

---

### `automatic_add_feature_update`

Append a markdown progress note to a feature's update log.

```
project:    string        — project name
feature_id: string        — UUID
content:    string        — markdown text (required)
author:     string (opt)  — agent id or name
```

Updates are append-only and timestamped. They appear in the Automatic UI so the user can follow what you are doing without asking.

**When to use:** After each significant unit of work — a decision made, a blocker found, a sub-task completed. Be specific. Bad: *"Made progress."* Good: *"Implemented JWT validation in `src/auth/middleware.ts`. Chose HS256 over RS256 because no external verifier is needed."*

---

### `automatic_delete_feature`

Permanently delete a feature and all its updates.

```
project:    string  — project name
feature_id: string  — UUID
```

**This cannot be undone.** Prefer archiving over deletion. Only delete features that were created by mistake or that contain no useful history.

---

## Standard Agent Workflow

Follow this sequence for every feature-driven session:

### 1. Orient

```
automatic_list_features(project: "my-project", state: "todo")
```

Identify the highest-priority feature to work on. If nothing is in `todo`, check `backlog` and ask the user which to start. Archived features will not appear — that is intentional.

### 2. Read the specification

```
automatic_get_feature(project: "my-project", feature_id: "<id>")
```

Read the full description and all prior updates. Do not start work until you understand the full scope. Check the `Archived` field in the response — if `true`, confirm with the user before proceeding.

### 3. Claim it

```
automatic_set_feature_state(project: "my-project", feature_id: "<id>", state: "in_progress")
automatic_update_feature(project: "my-project", feature_id: "<id>", assignee: "claude-code")
```

Move to `in_progress` and set `assignee` to your agent identity before touching any code. This signals to the user (and other agents) that the work is active and who is responsible for it. Use the same identifier consistently across all calls in the session (e.g. `"claude-code"`, `"cursor"`, `"gpt-4o"`).

### 4. Work and log

As you work, append updates after each meaningful step:

```
automatic_add_feature_update(
  project: "my-project",
  feature_id: "<id>",
  content: "Investigated the auth module. The existing `TokenService` at `src/services/token.ts` handles issuance but not validation. Will extend it rather than creating a new class.",
  author: "claude-code"
)
```

Log at minimum:
- After investigating the codebase (what you found, what approach you chose)
- When you hit a blocker
- After completing a significant sub-task
- Before any major decision point

### 5. Request review

```
automatic_set_feature_state(project: "my-project", feature_id: "<id>", state: "review")
automatic_add_feature_update(
  project: "my-project",
  feature_id: "<id>",
  content: "Implementation complete. Changes: `src/auth/middleware.ts` (new validation), `src/routes/api.ts` (middleware applied), `tests/auth.test.ts` (6 new tests, all passing). Ready for review.",
  author: "claude-code"
)
```

Always move to `review`, never to `complete`. The user marks features complete after they verify the work.

### 6. Capture discovered work

If you find additional work that was not part of the original feature:

```
automatic_create_feature(
  project: "my-project",
  title: "Refresh token expiry not enforced",
  description: "Found during auth implementation. Refresh tokens are issued without an expiry check on use...",
  priority: "high",
  created_by: "claude-code"
)
```

Do not silently fix unscoped work. Create a feature for it so the user is aware.

---

## Rules

- **Always set assignee** — when claiming a feature, immediately call `automatic_update_feature` with your agent identity in `assignee`. Never leave a feature `in_progress` without an assignee.
- **Never mark a feature `complete`** — that is the user's call after reviewing your work.
- **Always log updates** — the user should be able to read the update history and understand exactly what you did and why, without asking.
- **One feature at a time** — move a feature to `in_progress` before starting it. Do not have multiple features `in_progress` simultaneously.
- **Read before starting** — always call `automatic_get_feature` before beginning work so you have the full specification and prior context.
- **Capture, don't silently fix** — if you discover unscoped work, create a backlog feature for it rather than doing it without the user's knowledge.
- **Archived features are excluded by default** — `list_features` never returns archived features unless `include_archived: true` is passed. Do not assume a missing feature has been deleted; it may be archived.
