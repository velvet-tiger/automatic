# Agent Plugin Packages

An Agent Plugin package is a folder in the [Agent Plugins 1.0.0](https://agent-plugins.org/specification) format. Automatic can import one into its library in one step. The package's skills become library skills. Its MCP servers become library MCP servers.

Import only. Automatic does not export packages yet.

## Where it is in the app

Skills → Import → Local File.

- **Select Folder** picks a folder. If the folder has `plugin.json` at its root, Automatic imports it as an Agent Plugin package. Otherwise it runs the normal skill import.
- **Select File** also accepts a `plugin.json` file. Automatic imports the folder that holds it.
- The dialog lists the packages found in `~/.agents/plugins`, one level deep. Each has an Import button.

The result lists the imported skills and servers, the items that were skipped with a reason, and any warnings.

Code: `src-tauri/src/core/agent_package.rs`. Tauri commands: `is_agent_package_dir`, `import_agent_package`, `list_installed_agent_packages`.

## What is read

| Package part | Rule |
|---|---|
| `plugin.json` | Required. A closed object. `$schema` must be `https://agent-plugins.org/schemas/1.0.0/plugin.schema.json`. `name` is 1-64 characters from `a-z 0-9 . -`, starts and ends with a letter or digit, and has no `--` or `..`. |
| Unknown `plugin.json` field | Warning. The field is ignored. An inline `mcpServers` here is an unknown field and is not imported. |
| `extensions` that is not an object | Warning. Ignored. |
| Any other `plugin.json` problem | The whole package is rejected. Nothing is imported. |
| `skills/` | Each immediate child folder with a regular file named exactly `SKILL.md` is one skill. There is no deeper search. A child folder without `SKILL.md` is skipped and reported. A `skills` file is reported, and the rest of the package still loads. |
| `mcp.json` | An object with only `$schema` (`https://agent-plugins.org/schemas/1.0.0/mcp.schema.json`) and `mcpServers`. Invalid JSON, a wrong schema, or an extra field disables MCP for the package. Skills still import. |
| One `mcpServers` entry | Must match exactly one type. An unknown type, an unknown field, or a field from another type skips that entry only. |

Every path must resolve inside the package folder after following symlinks. A path that escapes is rejected.

## How servers are mapped

Automatic does not start servers. It stores each config in the library and writes it into agent config files. Each config must therefore work from any directory.

### `stdio`

| Package field | Stored as |
|---|---|
| `command` | A bare name is kept. A `./` path becomes the absolute path of the file inside the package. Any other form is rejected. |
| `args` | `${PLUGIN_ROOT}` and `${PLUGIN_DATA}` are expanded. |
| `env` | Values are expanded. `PLUGIN_ROOT` and `PLUGIN_DATA` keys are rejected. Automatic then sets both. |
| `cwd` | Resolved to an absolute path. `./...`, `${PLUGIN_ROOT}/...` and `${PLUGIN_DATA}/...` are accepted. The default is the package folder. |

- `${PLUGIN_ROOT}` is the absolute, symlink-resolved package folder.
- `${PLUGIN_DATA}` is `<Automatic data folder>/agent-packages/<package name>/data`. Import creates it and never deletes it.
- Expansion is one pass. Text produced by a replacement is not expanded again. Other `${...}` text is left as written.

The stored config is `{"type": "stdio", "command", "args", "env", "cwd"}`.

**The package must stay where it is.** Stored stdio servers point into the package folder.

### `streamable-http` and `sse`

Stored as `{"type": "http" | "sse", "url", "headers"}`.

- `url` must be an absolute http or https URL with no user name, password or fragment.
- Plain `http` is allowed only for `localhost` or a loopback IP address.
- Header names are compared without case. The same name twice with different case is rejected.

## Saving

- Skills go through the normal single-skill import, including its security scan and companion-file checks.
- Servers go through the same steps as a remote-source install: security scan, ciphertext blanking, and the registry save that encrypts env values. A Docker config the sync guard would block is skipped.
- A name that already exists in the library is skipped and reported as a conflict. It is never overwritten.
- Imported skills are grouped in a collection named after the package.

## Limits

- Some agents ignore `cwd`. Automatic does not write it for OpenCode and Kilo, and Warp reads `working_directory` instead. Commands and paths are absolute, so most servers still work.
- Two packages with the same name share one `${PLUGIN_DATA}` folder.
- An empty env value is treated by Automatic as "inherit from the shell", as for any other server.
- Client extension folders and `extensions` contents are ignored.
- Re-importing a package does not update items that are already in the library.
