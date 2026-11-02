#!/usr/bin/env bash
# A user of several agents with many projects to discover.
set -euo pipefail

bash "$(dirname "$0")/claude-user.sh"

mkdir -p "$HOME/.codex" "$HOME/.cursor"

for name in api web mobile docs; do
  mkdir -p "$HOME/code/$name"
  echo "# $name" > "$HOME/code/$name/AGENTS.md"
done
