#!/usr/bin/env bash
# Write the licences of the Rust crates used to build the app for each release
# platform.
#
# Usage: scripts/third-party-licenses.sh <output-dir>
#
# Writes <output-dir>/<release-name>.txt for every `name` in the build matrix
# of .github/workflows/release.yml. The release workflow ships each file in
# its app bundle as licenses/rust-crates.txt.
#
# Needs cargo-about 0.9.2 (`cargo install cargo-about --locked --features cli
# --version 0.9.2`). It reads about.toml and fails when a dependency's licence
# is not accepted there.
set -euo pipefail

out_dir=${1:?usage: third-party-licenses.sh <output-dir>}
root=$(cd "$(dirname "$0")/.." && pwd)

# Crates that publish no licence file with a copyright line, here or in
# their repository. cargo-about prints a generic licence text for them, with a
# placeholder where the copyright line belongs. That is the best there is.
# The objc2 crates (macOS) point to licences without including them.
# Any other crate printed that way has a licence file that cargo-about did not
# recognise, and needs a `clarify` entry in about.toml.
no_licence_file=" linux-keyutils block2 dispatch2 objc2 objc2-app-kit objc2-core-foundation objc2-encode objc2-exception-helper objc2-foundation objc2-osa-kit objc2-web-kit "

separator=$(printf '=%.0s' $(seq 1 80))

mkdir -p "$out_dir"
log=$(mktemp)
trap 'rm -f "$log"' EXIT

# Crates whose `clarify` entry in about.toml joins two licences with AND: the
# crate's own, and that of something it bundles.
two_licence_crates=$(awk '
  /^\[.*\.clarify\]$/ { crate = substr($0, 2, length($0) - 10); next }
  /^license = ".* AND .*"$/ && crate != "" { print crate }
  /^\[/ { crate = "" }
' "$root/about.toml")

# generate <release-name> <cargo-about build flags...>
#
# The flags must match the targets release.yml builds that release for, or
# the file will list the wrong crates.
generate() {
  local name=$1
  shift
  local out="$out_dir/$name.txt"
  local status=0

  cargo about generate \
    --locked \
    --fail \
    "$@" \
    --manifest-path "$root/src-tauri/Cargo.toml" \
    --config "$root/about.toml" \
    --output-file "$out" \
    "$root/about.hbs" >"$log" 2>&1 || status=$?
  cat "$log" >&2
  if [ "$status" -ne 0 ]; then
    echo "$name: cargo-about failed" >&2
    exit "$status"
  fi

  # When a file named in about.toml no longer matches its checksum,
  # cargo-about only warns. It then prints the crate as if the entry were not
  # there and exits 0, so a bundled library's licence would go missing from
  # the release without anyone noticing.
  if grep -q 'failed to validate all files specified in clarification' "$log"; then
    echo "$name: a licence file named in about.toml has changed. Check it and update its checksum." >&2
    exit 1
  fi

  # The same check from the other side, in case a later cargo-about words that
  # warning differently. A crate that is not built for this release is not
  # listed at all, which is fine.
  local crate listed
  for crate in $two_licence_crates; do
    listed=$(grep -c "^  $crate " "$out" || true)
    if [ "${listed:-0}" -eq 1 ]; then
      echo "$name: '$crate' is listed under one licence, expected its own and the one about.toml adds" >&2
      exit 1
    fi
  done

  # Sections that still hold the generic text's placeholder.
  local placeholders
  placeholders=$(awk -v separator="$separator" '
    function flush() { if (placeholder) print crates; crates = ""; placeholder = 0 }
    $0 == separator { flush(); next }
    /^  [^ ]+ [^ ]+  https:\/\/crates\.io\// { crates = crates " " $1; next }
    /<year>|<copyright holders>|<owner>/ { placeholder = 1 }
    END { flush() }
  ' "$out")
  for crate in $placeholders; do
    case "$no_licence_file" in
      *" $crate "*) ;;
      *)
        echo "$name: '$crate' is printed with a placeholder copyright line. Add a clarify entry for its licence file to about.toml, or add it to no_licence_file in this script if it publishes none." >&2
        exit 1
        ;;
    esac
  done

  if ! grep -q '^Used by:$' "$out"; then
    echo "$name: '$out' lists no crates" >&2
    exit 1
  fi
  echo "wrote $out" >&2
}

generate darwin-universal --target aarch64-apple-darwin --target x86_64-apple-darwin
generate linux-x86_64 --target x86_64-unknown-linux-gnu
generate windows-x86_64 --target x86_64-pc-windows-msvc
