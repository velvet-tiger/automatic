# Testing a first run

`make fresh` runs the app as a first-time user. It uses a throwaway home folder and deletes that folder when the app closes. Your real settings, projects and agent configs are not visible to the app.

```bash
make fresh
```

```bash
make fresh PROFILE=heavy-user
```

Keep the folder after the app closes:

```bash
KEEP=1 make fresh PROFILE=claude-user
```

Stop `make dev` first. Both use port 1421, and `make fresh` exits with an error if the port is in use.

## Profiles

Each profile is a script in `scripts/fresh-profiles/` that fills the empty home folder.

| Profile | User |
|---|---|
| `bare` | No agent configs and no projects |
| `claude-user` | Claude Code config, one global MCP server, one skill, one project in `~/code` |
| `heavy-user` | `claude-user`, plus Codex and Cursor config folders and four more projects |

## What changes in a fresh run

- `HOME` and `CFFIXED_USER_HOME` point at the throwaway folder. This moves settings, agent configs, secrets and WebKit `localStorage`.
- `PATH` holds system folders only, and the app skips its login shell probe. Agent CLIs on your machine are not detected.
- Newsletter calls and analytics events are skipped and logged. Your `.env` may hold a real Attio key, and the wizard would otherwise create a real contact.

cargo, npm and Vite keep your real home, so hot reload works. Only the app process is redirected, through `scripts/fresh-runner.sh`.

The guard is `is_fresh_run()` in `src-tauri/src/core/paths.rs`. Release builds ignore it.

## Limits

- **Installed apps are still detected.** Several agents check fixed paths such as `/Applications/Cursor.app`, so `bare` shows those agents if they are installed on your Mac.
- **This is a debug build.** It does not test the installer, notarization, Gatekeeper or the macOS keychain. Use a second macOS user or a VM for those before a release.
- **Other network calls still run,** such as the update check and marketplace requests.
