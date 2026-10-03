# Profiles

A profile is the live counterpart of a project template. A template is applied once and forgotten. A profile stays attached: saving it brings every attached project back in step and re-syncs them.

## What a profile holds

Library references only: `skills`, `mcp_servers`, `providers`, `agents`, `user_agents` (sub-agents), `user_commands`, `hooks`, and `rules`. No instruction text and no project files; those stay template-only because they cannot be kept live.

Stored at `~/.automatic/library/profiles/{name}.json` (`~/.automatic-dev/` in debug builds). The `name` field always follows the file stem.

## How a profile reaches a project

Profiles write their references into the project's own lists in `.automatic.json`. Nothing else in Automatic changes: save triggers sync, drift compares the lists with disk, autodetect finds the entries already present, and the `sync_projects_referencing_*` sweeps see them.

Two fields on `Project` carry the link:

- `profiles`: attached profile names, in attach order.
- `profile_contributions`: per profile, the entries that profile provides. Every entry the profile lists is recorded, whether the profile added it or the project already had it. Entries no attached profile lists stay the project's own.

`core::reconcile_project_profiles` is the one operation. It runs on every `save_project` command, on attach and detach, and in the sweep that follows a profile save, delete, or rename:

1. Profiles recorded in `profile_contributions` but no longer in `profiles` are detached. Their recorded entries are removed.
2. For each attached profile: entries it no longer lists are removed; every entry it lists is recorded as its contribution. Missing entries are added to the project. Entries the project already had are adopted in place, keeping their position and spelling.
3. An entry another attached profile already records stays with that profile. An entry another attached profile still lists is never removed. Its record moves to that profile.
4. `rules` go to `file_rules["_project"]`. MCP server names match without case.

A profile whose file is missing is reported and otherwise ignored, so nothing disappears because a file went missing.

## What this means in practice

- A profile owns every entry it lists on an attached project, including entries the project had before the profile was attached. Detaching the profile removes all of them. Entries no attached profile lists are the project's own and are never touched.
- Removing a profile-owned entry through a path that does not lock it (an MCP tool, a hand edit of `.automatic.json`) is undone on the next save. Edit or detach the profile instead.
- Deleting a library asset prunes it from every profile as well as every project. Renaming an MCP server or command renames it in profiles too.
- Per-project disabling of a profile-provided MCP server is not available. The editor hides the toggle on inherited servers.

## Group profiles

A project group can list profiles. Every member project receives them. A group may list several.

Two fields carry the link:

- `ProjectGroup.profiles`: profile names attached to the group, stored in `~/.automatic/groups/{name}.json`.
- `Project.group_profile_contributions`: per group, the entries in the project's `profiles` list that the group provides.

The group writes profile names into the project's own `profiles` list. There is no overlay at resolve time. From there the profile behaves like one attached by hand.

`core::reconcile_group_profiles` brings `profiles` in step with the project's groups. It always runs before `core::reconcile_project_profiles`. The first decides which profiles are attached. The second turns them into entries.

1. A group the project no longer belongs to releases every profile it recorded.
2. A member group releases profiles it no longer lists and records every profile it lists. A missing profile is added. A profile the project already had is adopted.
3. A profile another member group already records stays with that group. A released profile another member group still lists is kept, and its record moves to that group.

A released profile leaves `profiles` only. The profile reconcile that follows removes its entries.

Ownership:

- A group owns every profile it lists on a member, including one the project attached itself before. Removing it from the group removes it from the project.
- A group-provided profile cannot be detached on the project. `detach_profile_from_project` and `automatic_detach_profile` refuse and name the group.
- `group_profile_contributions` is machine-local state. Groups exist only on this machine, so the record lives in `.automatic/project.json`, not in `.automatic.json`. The profile name itself stays in the committed `profiles` list.

When it runs:

- `save_project` reconciles group profiles on every save.
- `save_group` and `delete_group` reconcile every project that was a member before or is a member after, and re-sync the ones that changed. Group edits did not sync projects before.
- Deleting a profile removes it from every group. Renaming a profile renames it in every group and in every record.
- Deleting or renaming a project needs no sweep. Group membership follows the project `id`.

Tauri commands: `attach_profile_to_group(group_name, profile_name)`, `detach_profile_from_group(group_name, profile_name)`, `get_groups_referencing_profile(profile_name)`. Attach fails when the profile or the group does not exist. Attach and detach are idempotent and re-sync the members that changed.

## In the app

- Library, Profiles: create and edit profiles, see which projects use one, attach one to a project.
- Project editor, Configuration, Profiles: attach and detach profiles for one project.
- Entries a profile provides carry a `Profile: name` badge and have no remove, edit, or toggle control in the Skills, MCP, Rules, Hooks, Agents, Commands, and Providers tabs.

## MCP tools

`automatic_list_profiles`, `automatic_read_profile`, `automatic_attach_profile`, `automatic_detach_profile`. `automatic_read_project` returns `profiles`, `profile_contributions` and `group_profile_contributions`.

Attach and detach take `project`, `group`, or neither for the current project. On a project they do not sync; call `automatic_sync_project` afterwards. On a group they reconcile every member and sync the ones that changed. Detaching a group-provided profile from a project is refused.

## Not yet covered

- Cloud library sync does not carry profiles. The sync contract spans the webapp and needs a change on both sides.
- The `automatic-cli` engine on `2.0-dev` has not received this work yet.
- Cloud library sync does not carry a group's `profiles` either.
- The rule migrations in `core/rules.rs` do not walk profile files.
