#!/bin/sh
# Cargo runner for `make fresh`. Starts the app with the throwaway home and a
# system-only PATH, so tools installed for the developer are not detected.
#
# CFFIXED_USER_HOME moves what macOS frameworks treat as home, which HOME alone
# does not do. WebKit keeps localStorage there.
set -eu

exec env \
  HOME="$AUTOMATIC_FRESH_HOME" \
  CFFIXED_USER_HOME="$AUTOMATIC_FRESH_HOME" \
  PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
  AUTOMATIC_FRESH_RUN=1 \
  "$@"
