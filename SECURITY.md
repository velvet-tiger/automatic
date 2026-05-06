# Security Policy

## Supported Versions

Automatic is distributed as a desktop application with built-in auto-update. Only the latest minor release line receives security fixes.

| Version | Supported          |
|---------| ------------------ |
| 1.x     | :white_check_mark: |
| < 1.0   | :x:                |

If you are on an older version, update through the in-app updater or download the [latest release](https://github.com/velvet-tiger/automatic/releases/latest).

## Reporting a Vulnerability

**Do not** open a public GitHub issue for security vulnerabilities.

Please report suspected vulnerabilities through one of the following channels:

- **Email:** support@velvettiger.com.au

When reporting, please include:

- A description of the vulnerability and its potential impact
- Steps to reproduce, including the affected version and platform
- Any proof-of-concept code, logs, or screenshots
- Whether the issue has been disclosed elsewhere

_Automatic will not pay for bug submissions_.

### What to Expect

- **Acknowledgement** within 3 business days of your report
- **Initial assessment** within 7 business days, including a severity rating and an indication of whether the report is accepted, needs more information, or is declined
- **Status updates** at least every 14 days while the issue is open
- **Fix and disclosure** coordinated with the reporter; we aim to ship a patched release within 30 days for high-severity issues

If a report is declined, we will explain why. If accepted, we will credit the reporter in the release notes unless anonymity is requested.

## Scope

In scope:

- The Automatic desktop application (Tauri binary, Rust backend, React frontend)
- The bundled MCP server (`mcp-serve` mode) and its tool surface
- Configuration handling under `~/.agents/` and project sync logic
- Auto-updater and code-signing pipeline
- Bundled skills, rules, and templates shipped in releases

Out of scope:

- Vulnerabilities in third-party MCP servers configured by users
- Vulnerabilities in agent tools (Claude Code, Codex CLI, Cursor, etc.) that consume Automatic's output
- Issues requiring physical access to an unlocked machine
- Social engineering of maintainers or users
- Denial-of-service through resource exhaustion on the local machine
