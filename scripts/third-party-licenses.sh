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

# Crates that publish no licence file with a copyright line. cargo-about
# prints a generic licence text for them, with a placeholder where the
# copyright line belongs. That is accepted, as in automatic-cli. brotli ships
# its MIT text but not its BSD-3-Clause one. Any other crate printed that way
# has a licence file that cargo-about did not recognise, and needs a
# `clarify` entry in about.toml.
no_licence_file=" alloc-stdlib block2 brotli dispatch2 dlopen2 dlopen2_derive linux-keyutils objc2 objc2-app-kit objc2-core-foundation objc2-encode objc2-exception-helper objc2-foundation objc2-osa-kit objc2-web-kit tauri-plugin tiktoken-rs unic-char-property unic-char-range unic-common unic-ucd-ident unic-ucd-version webview2-com webview2-com-macros webview2-com-sys "

# THIRD_PARTY_NOTICES.md names the webview2-com-sys version whose Microsoft
# WebView2 loader is linked into the Windows build. A different version may
# carry another SDK, so its notice must be checked when the crate changes.
webview2_version=$(awk '$0 == "name = \"webview2-com-sys\"" { getline; gsub(/version = |"/, ""); print }' "$root/src-tauri/Cargo.lock")
if [ -z "$webview2_version" ] || ! grep -q "\`webview2-com-sys\` $webview2_version " "$root/THIRD_PARTY_NOTICES.md"; then
  echo "THIRD_PARTY_NOTICES.md does not name webview2-com-sys '${webview2_version:-?}', the version in Cargo.lock. Check which WebView2 SDK it bundles and update the notice." >&2
  exit 1
fi

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

  # Any other warning or error fails the run too, such as a `git` clarify
  # entry whose file could not be fetched. cargo-about reports problems both
  # as "warning: ..." diagnostics and as timestamped "[WARN]" or "[ERROR]"
  # log lines. The only expected one is for about.toml entries that match no
  # crate built for this release.
  local unexpected
  unexpected=$(grep -E '^(warning|error)(\[[^]]*\])?:|\[[^]]*(WARN|ERROR)[^]]*\]' "$log" |
    grep -v -F 'warning: config found for a crate not present in the graph' || true)
  if [ -n "$unexpected" ]; then
    echo "$name: cargo-about reported a problem, shown in its output above:" >&2
    echo "$unexpected" >&2
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
