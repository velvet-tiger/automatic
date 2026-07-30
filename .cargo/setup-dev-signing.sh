#!/bin/sh
# One-time setup for the local development code-signing identity.
#
# Every macOS dev build is signed ad-hoc by the linker, and the resulting
# signature changes whenever the binary changes. macOS records that signature in
# the access control list of each keychain item the app reads, so clicking
# "Always Allow" only authorises the build that is running right now. The next
# build is a different program as far as the keychain is concerned, and the
# password dialog comes back.
#
# Signing with a real certificate records a designated requirement instead of a
# hash. That requirement stays the same across rebuilds, so a single
# "Always Allow" holds. This script provides the certificate that
# .cargo/sign-dev-binary.sh uses on every build.
#
# Two things have to be true, and this script ensures both:
#   1. An identity named automatic-dev exists.
#   2. codesign is allowed to use its private key without asking each time.
#
# Safe to re-run. See docs/code-signing.md.

set -eu

IDENTITY="automatic-dev"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "Dev signing applies to macOS only. Nothing to do on $(uname -s)."
    exit 0
fi

# A self-signed certificate has no trust chain, so it is reported as an invalid
# identity. codesign still accepts it, so match against the full identity list
# rather than the "-v" (valid only) subset.
identity_exists() {
    security find-identity -p codesigning | grep -q "\"$IDENTITY\""
}

# ── 1. Certificate ───────────────────────────────────────────────────────────

if identity_exists; then
    echo "Signing identity '$IDENTITY' is already present:"
    security find-identity -p codesigning | grep "\"$IDENTITY\""
else
    WORK="$(mktemp -d)"
    trap 'rm -rf "$WORK"' EXIT INT TERM

    # Transport password for the intermediate PKCS#12 file, which exists only
    # for the duration of the import below.
    TRANSPORT_PW="$(openssl rand -hex 16)"

    echo "Creating a self-signed code-signing certificate named '$IDENTITY'."

    # extendedKeyUsage=codeSigning is required. Without it macOS creates the
    # certificate but never offers it as a code-signing identity.
    cat > "$WORK/openssl.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions    = v3
prompt             = no

[dn]
CN = $IDENTITY

[v3]
basicConstraints     = critical,CA:false
keyUsage             = critical,digitalSignature
extendedKeyUsage     = critical,codeSigning
EOF

    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
        -keyout "$WORK/key.pem" \
        -out "$WORK/cert.pem" \
        -config "$WORK/openssl.cnf" >/dev/null 2>&1

    # OpenSSL 3 defaults to AES-256-CBC with a SHA-256 MAC, which the macOS
    # keychain importer rejects with "MAC verification failed during PKCS12
    # import". The explicit legacy algorithms below are what `security import`
    # can actually read.
    openssl pkcs12 -export \
        -out "$WORK/identity.p12" \
        -inkey "$WORK/key.pem" \
        -in "$WORK/cert.pem" \
        -name "$IDENTITY" \
        -passout "pass:$TRANSPORT_PW" \
        -certpbe PBE-SHA1-3DES \
        -keypbe PBE-SHA1-3DES \
        -macalg sha1 >/dev/null 2>&1

    echo "Importing it into your login keychain."
    security import "$WORK/identity.p12" \
        -k "$KEYCHAIN" \
        -P "$TRANSPORT_PW" \
        -T /usr/bin/codesign \
        -A >/dev/null

    if ! identity_exists; then
        echo "Setup failed: '$IDENTITY' is not present after import." >&2
        echo "See docs/code-signing.md." >&2
        exit 1
    fi
fi

# ── 2. Authorise codesign to use the key ─────────────────────────────────────
#
# This runs even when the certificate already existed. Having the identity is
# not enough: without this, codesign finds the key but macOS still asks for
# permission to use it on every build, which trades one dialog for another.
#
# -l scopes the change to this key by label. Omitting it would rewrite the
# partition list of every signing key in the login keychain, including any
# Developer ID keys used for release builds.

echo
echo "macOS will now ask for your login keychain password. This authorises"
echo "codesign to use the '$IDENTITY' key without prompting on every build."
echo

security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s -l "$IDENTITY" \
    "$KEYCHAIN" >/dev/null

# ── 3. Prove it works ────────────────────────────────────────────────────────

PROBE="$(mktemp -d)"
trap 'rm -rf "${WORK:-}" "$PROBE"' EXIT INT TERM
cp /usr/bin/true "$PROBE/probe"
chmod u+w "$PROBE/probe"

echo "Verifying that codesign can sign without prompting."
if codesign --force --sign "$IDENTITY" --identifier com.velvet.automatic.dev "$PROBE/probe" 2>/dev/null; then
    echo
    echo "Done. Signing works. Designated requirement:"
    codesign -d -r- "$PROBE/probe" 2>&1 | grep "designated"
    echo
    echo "Next: run 'make dev'. macOS asks once per stored secret, because each"
    echo "item still lists the previous unsigned build. Choose 'Always Allow'"
    echo "for each. Later builds reuse this identity and will not ask again."
else
    echo
    echo "codesign could not use the key." >&2
    echo "Re-run this script, or see docs/code-signing.md." >&2
    exit 1
fi
