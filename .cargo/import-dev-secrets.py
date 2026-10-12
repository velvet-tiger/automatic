#!/usr/bin/env python3
"""Copy the dev keychain items into the dev secrets file. macOS only.

Debug builds used to keep their secrets in the macOS keychain under the
`automatic_desktop_dev` service. They now keep them in
`~/.automatic-dev/dev-secrets.json` (see src-tauri/src/core/secret_store.rs).
This script copies the existing items across, so the env-encryption key and the
stored API keys and OAuth tokens keep working.

Run it once with `make dev-secrets-import`, before the first run of a debug
build that uses the file. macOS asks for permission once per item, because
this is the last time anything reads them. The keychain items are left in
place.

An account that is already in the file is left alone, so a second run only
fetches what is missing. Pass `--overwrite` to replace file values with the
keychain values.

This is Python because the secrets are written as JSON, and an OAuth
credential is itself a JSON string that needs correct escaping.
"""

import argparse
import fcntl
import json
import os
import re
import subprocess
import sys
from pathlib import Path

# The absolute path keeps a `security` earlier on PATH from seeing the secrets.
SECURITY = "/usr/bin/security"
SERVICE = "automatic_desktop_dev"
DEFAULT_OUTPUT = Path.home() / ".automatic-dev" / "dev-secrets.json"

ATTRIBUTE = re.compile(r'^\s*"(acct|svce)"<blob>=(.*)$')


def decode_security_value(printed):
    """Decode a value as the `security` tool prints it.

    A value made only of printable ASCII, with no backslash, is printed raw
    inside double quotes. Any other value is printed as `0x<HEX>`, followed by
    a quoted form that is for reading only. The hex is the exact value.
    Returns None for an empty or missing value.
    """
    printed = printed.strip()
    if printed in ("", "<NULL>"):
        return None
    hex_form = re.match(r"^0x([0-9A-Fa-f]+)(\s|$)", printed)
    if hex_form:
        return bytes.fromhex(hex_form.group(1)).decode("utf-8")
    if len(printed) >= 2 and printed.startswith('"') and printed.endswith('"'):
        return printed[1:-1]
    raise ValueError("unrecognised value format from the security tool")


def accounts_in_keychain_dump(dump):
    """Return the account names stored under SERVICE, from `dump-keychain` output."""
    accounts = []
    record = {}

    def close_record():
        if record.get("svce") == SERVICE and record.get("acct"):
            accounts.append(record["acct"])
        record.clear()

    for line in dump.splitlines():
        if line.startswith("keychain:"):
            close_record()
            continue
        attribute = ATTRIBUTE.match(line)
        if attribute:
            # The dump covers every app's items. One that this script cannot
            # decode is not one of ours, so it must not stop the import.
            try:
                record[attribute.group(1)] = decode_security_value(attribute.group(2))
            except ValueError:
                record[attribute.group(1)] = None
    close_record()
    return sorted(set(accounts))


def list_accounts():
    # Without `-d` this prints attributes only. It reads no secret and raises
    # no dialog.
    dump = subprocess.run([SECURITY, "dump-keychain"], capture_output=True, text=True)
    if dump.returncode != 0:
        sys.exit("Could not list the keychain: `security dump-keychain` exited with %d." % dump.returncode)
    return accounts_in_keychain_dump(dump.stdout)


def read_secret(account):
    """Read one secret. Raises RuntimeError when macOS refuses or the item is gone."""
    found = subprocess.run(
        [SECURITY, "find-generic-password", "-s", SERVICE, "-a", account, "-g"],
        capture_output=True,
        text=True,
    )
    if found.returncode != 0:
        raise RuntimeError("the security tool exited with %d" % found.returncode)
    # `-g` prints the attributes on stdout and the password line on stderr.
    for line in found.stderr.splitlines():
        if line.startswith("password: "):
            secret = decode_security_value(line[len("password: "):])
            if secret is None:
                raise RuntimeError("the item is empty")
            return secret
    raise RuntimeError("the security tool printed no password line")


def read_existing(path):
    """Read the secrets file. Raises ValueError when it is not a JSON object of strings."""
    try:
        raw = path.read_text()
    except FileNotFoundError:
        return {}
    try:
        secrets = json.loads(raw)
    except json.JSONDecodeError as error:
        # The position only. The parser's message can quote the file.
        raise ValueError(
            "%s is not valid JSON (line %d, column %d)" % (path, error.lineno, error.colno)
        ) from None
    if not isinstance(secrets, dict) or not all(
        isinstance(value, str) for value in secrets.values()
    ):
        raise ValueError("%s is not a JSON object of strings" % path)
    return secrets


def lock_for_update(path):
    """Take the lock the debug build takes for a write (see secret_store.rs).

    Returns the open lock file. The lock is released when it is closed.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path.with_name(path.name + ".lock"), os.O_WRONLY | os.O_CREAT, 0o600)
    fcntl.flock(descriptor, fcntl.LOCK_EX)
    return os.fdopen(descriptor, "w")


def write_owner_only(path, secrets):
    """Write through an owner-only temp file, then rename it into place."""
    tmp = path.with_name("%s.%d.tmp" % (path.name, os.getpid()))
    try:
        os.unlink(tmp)
    except FileNotFoundError:
        pass
    descriptor = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor, "w") as handle:
            json.dump(secrets, handle, indent=2, sort_keys=True)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except FileNotFoundError:
            pass
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help="secrets file to write (default: %(default)s)",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="replace accounts that are already in the file with the keychain values",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="print the account names that would be imported, and read no secret",
    )
    args = parser.parse_args()

    if sys.platform != "darwin":
        sys.exit("This script reads the macOS keychain. There is nothing to import here.")

    accounts = list_accounts()
    if not accounts:
        print("No '%s' items found in the keychain. Nothing to import." % SERVICE)
        return
    if args.list:
        print("\n".join(accounts))
        return

    try:
        present = set(read_existing(args.output))
    except ValueError as error:
        sys.exit(str(error))
    wanted = [a for a in accounts if args.overwrite or a not in present]
    for account in accounts:
        if account not in wanted:
            print("  kept      %s (already in the file; --overwrite replaces it)" % account)
    if not wanted:
        print("Nothing to import.")
        return

    # The keychain reads wait on dialogs, so they happen before the lock is
    # taken. A debug build that writes in the meantime is not blocked.
    print("Importing %d item(s). macOS will ask for permission for each one." % len(wanted))
    fetched = {}
    failed = []
    for account in wanted:
        try:
            fetched[account] = read_secret(account)
        except (RuntimeError, ValueError) as error:
            failed.append(account)
            print("  failed    %s (%s)" % (account, error))

    if fetched:
        with lock_for_update(args.output):
            try:
                secrets = read_existing(args.output)
            except ValueError as error:
                sys.exit(str(error))
            for account, secret in fetched.items():
                # Re-check under the lock: a debug build may have stored this
                # account while the dialogs were open.
                if account in secrets and not args.overwrite:
                    print("  kept      %s (stored by another process during the import)" % account)
                    continue
                print("  %s  %s" % ("replaced" if account in secrets else "imported", account))
                secrets[account] = secret
            write_owner_only(args.output, secrets)
        print("Wrote %s" % args.output)
    if failed:
        sys.exit("%d item(s) were not imported. Run the script again to retry them." % len(failed))


if __name__ == "__main__":
    main()
