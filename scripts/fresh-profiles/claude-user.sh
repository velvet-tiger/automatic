#!/usr/bin/env bash
# A Claude Code user with a global MCP server, a skill and one project.
set -euo pipefail

mkdir -p "$HOME/.claude/skills/commit-helper" "$HOME/code/my-app"

cat > "$HOME/.claude.json" <<'JSON'
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
    }
  }
}
JSON

cat > "$HOME/.claude/skills/commit-helper/SKILL.md" <<'MD'
---
name: commit-helper
description: Fixture skill for testing the first-run import.
---

Write short commit messages.
MD

echo "# My app" > "$HOME/code/my-app/CLAUDE.md"
