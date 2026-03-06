# Code Signing — Automatic Dev Build

## Overview

The `automatic-dev` self-signed certificate in `login.keychain` is used for local dev builds.
It is configured in `src-tauri/tauri.dev.conf.json`:

```json
{
  "bundle": {
    "macOS": {
      "signingIdentity": "automatic-dev"
    }
  }
}
```

The production build uses `"Developer ID Application: Christopher Skene (668BQY2X33)"` (configured in `src-tauri/tauri.conf.json`).

---

## Symptom: Repeated keychain password prompts

If macOS keeps asking for a keychain password during `tauri dev` or `tauri build`, the `automatic-dev` private key is missing from the keychain. The certificate exists as an orphan but cannot be used for signing.

---

## Diagnosis

**Safe check — no password prompts:**

```bash
security find-key -a 2>&1 | grep -i automatic
```

If this returns nothing, the private key is gone.

**Confirm the cert is orphaned:**

```bash
security find-identity -v
```

`automatic-dev` will be absent from the output if the private key is missing. Only identities with both a cert AND a private key appear here.

> Do NOT use `security dump-keychain -d` — it prompts for keychain access on every single item.

---

## Fix

Delete the orphaned cert and recreate the self-signed identity:

```bash
# 1. Delete the orphaned certificate
security delete-certificate -c "automatic-dev" ~/Library/Keychains/login.keychain-db

# 2. Generate a new key + self-signed cert
openssl req -x509 -newkey rsa:2048 -keyout automatic-dev.key \
  -out automatic-dev.crt -days 3650 -nodes \
  -subj "/CN=automatic-dev/C=AU/emailAddress=chris.skene@gmail.com"

# 3. Bundle into a PKCS#12
openssl pkcs12 -export -out automatic-dev.p12 \
  -inkey automatic-dev.key -in automatic-dev.crt -passout pass:

# 4. Import into login keychain
security import automatic-dev.p12 -k ~/Library/Keychains/login.keychain-db \
  -T /usr/bin/codesign -P ""

# 5. Set ACL so codesign can access the key without prompting
security set-key-partition-list -S apple-tool:,apple:,codesign: \
  -s -k "" ~/Library/Keychains/login.keychain-db

# 6. Clean up temp files
rm automatic-dev.key automatic-dev.crt automatic-dev.p12

# 7. Verify
security find-identity -v | grep automatic-dev
```

---

## Known Causes of Key Loss

1. **Temporary build keychain cleaned up** — The CI release workflow (`release.yml`) creates a temporary `build.keychain`. If the `automatic-dev` private key ended up there instead of `login.keychain`, it would be lost when the temp keychain was deleted.
2. **macOS Ventura/Sonoma keychain migration** — Apple's keychain migration between OS versions can drop private keys, leaving orphaned certificates.
3. **ACL change after macOS update** — The access control list on the private key can change after a system update, causing codesign to lose access and fall back to prompting.
