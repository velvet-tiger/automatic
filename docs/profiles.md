# Profiles

A profile is the live counterpart of a project template. A template is applied once and forgotten. A profile stays attached: saving it brings every attached project back in step and re-syncs them.

## What a profile holds

Library references only: `skills`, `mcp_servers`, `providers`, `agents`, `user_agents` (sub-agents), `user_commands`, `hooks`, and `rules`. No instruction text and no project files; those stay template-only because they cannot be kept live.

Stored at `~/.automatic/library/profiles/{name}.json` (`~/.automatic-dev/` in debug builds). The `name` field always follows the file stem.

## How a profile reaches a project

Profiles write their references into the project's own lists in `.automatic/project.json`. Nothing else in Automatic changes: save triggers sync, drift compares the lists with disk, autodetect finds the entries already present, and the `sync_projects_referencing_*` sweeps see them.

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
- Removing a profile-owned entry through a path that does not lock it (an MCP tool, a hand edit of `project.json`) is undone on the next save. Edit or detach the profile instead.
- Deleting a library asset prunes it from every profile as well as every project. Renaming an MCP server or command renames it in profiles too.
- Per-project disabling of a profile-provided MCP server is not available. The editor hides the toggle on inherited servers.

## In the app

- Library, Profiles: create and edit profiles, see which projects use one, attach one to a project.
- Project editor, Configuration, Profiles: attach and detach profiles for one project.
- Entries a profile provides carry a `Profile: name` badge and have no remove, edit, or toggle control in the Skills, MCP, Rules, Hooks, Agents, Commands, and Providers tabs.

## MCP tools

`automatic_list_profiles`, `automatic_read_profile`, `automatic_attach_profile`, `automatic_detach_profile`. Attach and detach do not sync; call `automatic_sync_project` afterwards. `automatic_read_project` returns `profiles` and `profile_contributions`.

## Not yet covered

- Cloud library sync does not carry profiles. The sync contract spans the webapp and needs a change on both sides.
- The `automatic-cli` engine on `2.0-dev` has not received this work yet.
- The rule migrations in `core/rules.rs` do not walk profile files.
