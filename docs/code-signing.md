# Code Signing — Automatic Dev Build

## Overview

Two separate things get signed, for different reasons.

**Release builds** are signed by Tauri with `"Developer ID Application: Christopher Skene (668BQY2X33)"`, configured in `src-tauri/tauri.conf.json`. This is what ships.

**Dev builds** are signed with a local self-signed certificate named `automatic-dev`, applied by `.cargo/sign-dev-binary.sh`. This exists only to stop macOS asking for your keychain password after every build. It has nothing to do with distribution.

Run this once per machine:

```bash
make dev-signing-setup
```

---

## Why dev builds need signing at all

The app stores API keys, OAuth tokens, and the environment-variable encryption key in the macOS keychain, under the service name `automatic_desktop_dev` in debug builds (see `src-tauri/src/core/mod.rs`).

Each keychain item records which program is allowed to read it. For signed code, macOS records the *designated requirement*, which is derived from the code signing identifier and the certificate. For unsigned code it records a hash of the binary instead.

The linker gives every Rust build on Apple Silicon an ad-hoc signature, and its hash changes whenever the binary changes:

```
Format=Mach-O thin (arm64)
CodeDirectory v=20400 flags=0x20002(adhoc,linker-signed)
Signature=adhoc
TeamIdentifier=not set
```

So clicking "Always Allow" authorises the build that is running right now, and the next build is a different program as far as the keychain is concerned. The dialog comes straight back. Because every keychain read is a separate access check, a single session can raise the dialog dozens of times.

Signing with a fixed certificate replaces the hash with a designated requirement that does not change between rebuilds:

```
designated => identifier "com.velvet.automatic.dev" and certificate leaf = H"..."
```

One "Always Allow" per item then holds indefinitely.

---

## How it is wired up

`tauri dev` shells out to `cargo run`, and re-runs it on every change picked up by the file watcher. Cargo's `runner` setting intercepts that, so signing happens on every rebuild without any extra step.

`.cargo/config.toml`:

```toml
[target.aarch64-apple-darwin]
runner = ".cargo/sign-dev-binary.sh"
```

Cargo resolves that path against the config file, not the working directory, so it works even though cargo runs from `src-tauri/`.

The runner signs the binary and then execs it. If the certificate is not installed it prints a warning and runs unsigned, so a machine without the setup still builds. Test and benchmark binaries under `target/*/deps/` are skipped; they use an in-memory key rather than the keychain.

Release builds go through `cargo build`, which does not use `runner`, so none of this affects them.

---

## Setup

```bash
make dev-signing-setup
```

The script is idempotent. It creates the certificate if it is missing, always re-authorises `codesign` to use the key, and finishes by signing a throwaway binary to prove signing works without prompting.

It asks for your login keychain password once, at step 2. That authorises `codesign`, and is the step most likely to be missing on a machine that still prompts.

After setup, the first `make dev` still raises one dialog per stored secret, because each item was last authorised for an unsigned build. Choose **Always Allow** for each. Later builds reuse the same identity and stay silent.

---

## Diagnosis

**Is the identity present?**

```bash
security find-identity -p codesigning
```

Look for `automatic-dev`. It will be listed as `CSSMERR_TP_NOT_TRUSTED`, which is expected and harmless: the certificate is self-signed, so it has no trust chain. `codesign` accepts it regardless.

Do not use `security find-identity -v` for this check. `-v` filters to valid identities only and hides self-signed ones, which makes a working certificate look absent.

**Is the built binary actually signed?**

```bash
codesign -dvvv src-tauri/target/debug/automatic 2>&1 | grep -E "Identifier|Authority|Signature"
```

`Authority=automatic-dev` means the runner ran. `Signature=adhoc` means it did not.

**What requirement does the keychain see?**

```bash
codesign -d -r- src-tauri/target/debug/automatic
```

This should be identical before and after a rebuild. If it changes, "Always Allow" will not stick.

> Do not use `security dump-keychain -d`. It prompts for access to every single item.

---

## Troubleshooting

**A dialog says something wants to sign using key "automatic-dev".**

`codesign` is not authorised to use the private key. Choose "Always Allow" once, or run `make dev-signing-setup` to set it non-interactively. Until one of those happens, the build blocks on the dialog rather than failing.

**The identity exists but signing still prompts.**

The key's partition list does not include `codesign`. Re-run `make dev-signing-setup`.

**`security import` fails with "MAC verification failed during PKCS12 import".**

OpenSSL 3 defaults to AES-256-CBC with a SHA-256 MAC, which the macOS keychain importer cannot read. Export with legacy algorithms:

```bash
openssl pkcs12 -export -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 ...
```

The setup script already does this.

**A newly created certificate never appears as a signing identity.**

The certificate is missing `extendedKeyUsage=codeSigning`. `openssl req -x509` does not add it unless an extensions section asks for it. The setup script supplies one.

**Never run `set-key-partition-list` without `-l`.**

Scoping matters:

```bash
# Correct: touches only the automatic-dev key
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -l automatic-dev ~/Library/Keychains/login.keychain-db

# Wrong: rewrites the partition list of EVERY signing key in the keychain,
# including the Developer ID keys used for release builds
security set-key-partition-list -S apple-tool:,apple:,codesign: -s ~/Library/Keychains/login.keychain-db
```

Omitting `-k` makes `security` prompt for the password interactively, which is preferable to passing it on the command line where it is visible to `ps`.

---

## Note on `tauri.dev.conf.json`

`src-tauri/tauri.dev.conf.json` sets `bundle.macOS.signingIdentity` to `automatic-dev`. That setting only applies to bundling, so it affects `tauri build --config src-tauri/tauri.dev.conf.json` and nothing else. `tauri dev` never bundles and never reads it. Dev-build signing is handled entirely by the cargo runner described above.
