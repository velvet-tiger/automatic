#!/bin/sh
# Cargo `runner` for macOS dev builds: sign, then run.
#
# The linker gives every build an ad-hoc signature derived from the binary
# contents, so each build presents a different identity to the macOS keychain.
# Keychain items remember which program was authorised, so "Always Allow" is
# void as soon as the binary is rebuilt, and the password dialog returns.
#
# Signing with a fixed certificate replaces the content hash with a designated
# requirement that does not change between builds, so the authorisation holds.
#
# `tauri dev` invokes `cargo run`, which means this also runs on every rebuild
# triggered by the file watcher.
#
# Run .cargo/setup-dev-signing.sh once to create the certificate.

set -eu

BIN="$1"
shift

IDENTITY="automatic-dev"

# The GUI and the CLI deliberately share one identifier. The keychain matches on
# the designated requirement, which is built from the identifier and the
# certificate, so a single "Always Allow" covers both binaries instead of
# needing one per binary.
IDENTIFIER="com.velvet.automatic.dev"

case "$BIN" in
    */deps/*)
        # Test and benchmark binaries. They use an in-memory key rather than the
        # keychain, so signing them would only cost build time.
        exec "$BIN" "$@"
        ;;
esac

# A self-signed certificate has no trust chain and is therefore reported as an
# invalid identity, so match the full list rather than the "-v" subset.
if security find-identity -p codesigning 2>/dev/null | grep -q "\"$IDENTITY\""; then
    if ! codesign --force --sign "$IDENTITY" --identifier "$IDENTIFIER" "$BIN" 2>/dev/null; then
        echo "warning: could not sign with '$IDENTITY'; running unsigned." >&2
        echo "         Expect a keychain prompt for each stored secret." >&2
    fi
else
    echo "warning: signing identity '$IDENTITY' not found; running unsigned." >&2
    echo "         macOS will prompt for keychain access after every build." >&2
    echo "         Run 'make dev-signing-setup' once to stop that." >&2
fi

exec "$BIN" "$@"
