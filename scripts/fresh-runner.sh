#!/bin/sh
# Cargo runner for `make fresh`. Starts the app with the throwaway home and a
# system-only PATH, so tools installed for the developer are not detected.
#
# CFFIXED_USER_HOME moves what macOS frameworks treat as home, which HOME alone
# does not do. WebKit keeps localStorage there.
#
# `env -i` starts from an empty environment. Inherited values such as
# AUTOMATIC_KEYCHAIN, CLINE_DIR and XDG_CONFIG_HOME would point the app back at
# real data. Add a variable below only if the app cannot start without it.
set -eu

exec env -i \
  HOME="$AUTOMATIC_FRESH_HOME" \
  CFFIXED_USER_HOME="$AUTOMATIC_FRESH_HOME" \
  PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
  AUTOMATIC_FRESH_RUN=1 \
  USER="${USER:-}" \
  LOGNAME="${LOGNAME:-}" \
  LANG="${LANG:-en_US.UTF-8}" \
  TMPDIR="${TMPDIR:-/tmp}" \
  DISPLAY="${DISPLAY:-}" \
  WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" \
  XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}" \
  "$@"
