# Project Import

A cloned repository can hold a committed `.automatic.json`. Most of its lists are names, and sync looks those names up in the user's library. A repository can therefore reach library entries by naming them. MCP `env` values matter most, because sync writes the decrypted values into agent files inside the repository folder.

## What the review shows

Add Project and Find Projects show which names match this machine's library, before the project is added. The user then adds it normally or in read-only mode. A read-only project is not synced, so nothing from the library is written into its folder. If the config sets the committed `read_only` hint, read-only is the first choice in Add Project and starts ticked in Find Projects.

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
| Providers | `providers` | Not matched. Each committed id is shown, because providers have no library entry and sync does not use them. |

File lookups follow the disk, so on a case-insensitive volume `Shared-Skill` matches `shared-skill`.

Hooks are flagged. So are MCP servers with stored `env` or `headers` values. A stored value counts even if it is empty, because values stay encrypted. An OAuth sign-in is not flagged, because checking for one needs the keychain. Those servers are still listed.

The review shows names and kinds only. A repository's names are untrusted: control and invisible characters are replaced, long names are shortened, and each kind shows at most 50 names. Each kind looks up at most 1,000 names, except MCP servers, and the review reports how many it skipped. A config that cannot be read shows no detail and cannot be added.

## What it does not cover

- **Later changes.** The review runs at import only. A `git pull` that adds a name is not reviewed, and neither sync nor drift detection asks.
- **Inline content.** `custom_skills`, `custom_agents`, `custom_commands` and `custom_rules` carry their own content and are not listed. The repository can also commit agent files directly.
- **Registries.** Opening an imported project, even in read-only mode, can copy committed `skill_sources` and `skill_collections` into the user's registries.
