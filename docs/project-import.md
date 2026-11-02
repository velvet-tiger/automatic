# Project Import

A cloned repository can hold a committed `.automatic.json`. Most of its lists are names, and sync looks those names up in the user's library. A repository can therefore reach library entries by naming them. MCP `env` values matter most, because sync writes the decrypted values into agent files inside the repository folder.

## What the review shows

Add Project and Find Projects show which names match this machine's library, before the project is added. The user then adds it normally or in read-only mode. A read-only project is not synced, so nothing from the library is written into its folder.

Read-only is the starting choice when the config sets the committed `read_only` hint, when the check failed or was skipped, when names were left unchecked, or when a named profile cannot be read. It is the primary button in Add Project and starts ticked in Find Projects. A failed check cannot read the hint, so it starts read-only for that reason too. Only the user can then pick normal mode. A folder that reaches Find Projects' add step with no recorded choice is added read-only. A direct import with no choice follows the hint.

`core::preview_project_library_use` builds the list. It writes nothing and decrypts nothing.

| Kind | Config field | How a name matches |
|---|---|---|
| MCP servers | `mcp_servers`, without `disabled_mcp_servers` and `automatic` | Exact match against the registry listing |
| Hooks | `hooks` | The file lookup sync uses |
| Skills | `skills`, without `automatic` | The file lookup sync uses, across every skill source |
| Sub-agents | `user_agents` | The file lookup sync uses |
| Commands | `user_commands` | The file lookup sync uses |
| Rules | every list in `file_rules` | The file lookup sync uses |
| Profiles | `profiles` | The profile file lookup |

Rules list every `file_rules` entry, so the list can name more than sync uses for one target file.

A matched profile adds to a project when it reconciles, so its hooks, MCP servers, skills, sub-agents, commands and rules are counted in the groups above, under the same limits. The project's `disabled_mcp_servers` also excludes a profile's servers.

File lookups follow the disk, so on a case-insensitive volume `Shared-Skill` matches `shared-skill`.

These items are flagged:

- Every hook.
- Every remote MCP server (`http` or `sse` type, or a `url`). A sign-in cannot be checked without the keychain, so the review assumes one may be stored.
- MCP servers with stored `env` or `headers` values. An entry counts unless the stored JSON holds null or an empty string. `env` values are stored encrypted, so an `env` entry saved empty still counts.
- Profiles that provide a matching hook or a flagged server, and profiles that cannot be read.
- MCP servers whose stored config cannot be read.

The review shows names and kinds only. A repository's names are untrusted: control and invisible characters are replaced, long names are shortened, and each kind shows at most 50 names. Each kind looks up at most 1,000 names, except MCP servers, and the review reports how many it skipped. A config that cannot be read shows no detail and cannot be added.

Find Projects previews at most 200 unregistered configs in one scan. A folder past that shows as not checked and starts read-only.

## What it does not cover

- **Later changes.** The review runs at import only. A `git pull` that adds a name is not reviewed, and neither sync nor drift detection asks.
- **Providers.** `providers` is not reviewed. Sync does not use it, nothing validates its ids, and it grants no access to the library.
- **Contexts.** The dialog lists the slugs in `contexts`, but `git pull` is not reviewed. A slug the config names is pending on each checkout. Agents cannot read it until the user attaches it in the project, or a group the checkout joined provides it.
- **Client working directory.** MCP context and project reads use the registered project containing the folder the server runs in, with no other repository between them. A linked worktree of the project reads as the project. After `automatic_register_project`, agents cannot read the project's contexts or project details until the user keeps it in the app, and it is nobody's peer. If that folder is inside no registered project and the client declares `roots`, the server asks for them on each call and applies the same rule. Every listed root must resolve to the same project, and a list over 64 refuses. A folder inside a registered project decides alone, and an unknown one refuses. Without either, or if `roots/list` fails or takes over two seconds, reads are refused, even in a registered project. Clients on protocol 2026-07-28 or later cannot be asked.
- **Inline content.** `custom_skills`, `custom_agents`, `custom_commands` and `custom_rules` carry their own content and are not listed. The repository can also commit agent files directly.
