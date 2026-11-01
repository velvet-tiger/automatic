#!/usr/bin/env bash
# Run the app as a first-time user, in a throwaway home folder.
#
# Usage: ./scripts/fresh-run.sh [bare|claude-user|heavy-user]
#        KEEP=1 keeps the folder after the app exits.
#
# Only the app process sees the throwaway home. cargo, npm and Vite keep the
# real one, so they still find their toolchains and hot reload still works.
# That is why this is a cargo runner and not `HOME=... npm run tauri dev`.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
profile="${1:-bare}"
profile_script="$repo_root/scripts/fresh-profiles/$profile.sh"
vite_port=1421

if [[ ! -f "$profile_script" ]]; then
  echo "error: unknown profile '$profile'. Available profiles:" >&2
  ls "$repo_root/scripts/fresh-profiles" | sed 's/\.sh$//' >&2
  exit 2
fi

# Cargo splits a runner value on whitespace, so the runner path cannot hold any.
if [[ "$repo_root" =~ [[:space:]] ]]; then
  echo "error: this checkout's path contains a space: $repo_root" >&2
  exit 1
fi

if lsof -nP -iTCP:"$vite_port" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "error: port $vite_port is in use, most likely by 'make dev'." >&2
  echo "Stop that server, then run this again." >&2
  exit 1
fi

fresh_home="$(mktemp -d "${TMPDIR:-/tmp}/automatic-fresh.XXXXXX")"

cleanup() {
  if [[ "${KEEP:-}" == "1" ]]; then
    echo "[fresh] kept $fresh_home"
  else
    rm -rf "$fresh_home"
  fi
}
trap cleanup EXIT

HOME="$fresh_home" bash "$profile_script"

echo "[fresh] profile: $profile"
echo "[fresh] home:    $fresh_home"

export AUTOMATIC_FRESH_HOME="$fresh_home"

# Without a runner the app would start in the real home, so stop if the host is
# unknown. This variable takes priority over .cargo/config.toml.
host="$(rustc -vV | sed -n 's/^host: //p')"
if [[ -z "$host" ]]; then
  echo "error: could not read the host target from rustc." >&2
  exit 1
fi
runner_var="CARGO_TARGET_$(echo "$host" | tr 'a-z-' 'A-Z_')_RUNNER"
export "$runner_var=$repo_root/scripts/fresh-runner.sh"

cd "$repo_root"
npm run tauri dev
