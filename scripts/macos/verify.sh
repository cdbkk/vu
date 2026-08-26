#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/macos/common.sh
source "$SCRIPT_DIR/common.sh"

setup_release_env

require_cmd codesign

cli_binary="$VU_APP_BUNDLE_PATH/Contents/MacOS/vu-cli"
if [[ ! -x "$cli_binary" ]]; then
  fail "vu-cli missing from app bundle: $cli_binary"
fi

terminfo_dir="$VU_APP_BUNDLE_PATH/Contents/Resources/terminfo"
if [[ ! -d "$terminfo_dir" ]]; then
  fail "Ghostty terminfo directory missing from app bundle: $terminfo_dir"
fi
terminfo_entry="$(find "$terminfo_dir" -type f -name xterm-ghostty -print -quit)"
if [[ -z "$terminfo_entry" || ! -r "$terminfo_entry" ]]; then
  fail "Ghostty xterm-ghostty terminfo entry missing from app bundle: $terminfo_dir"
fi

log "Verifying code signature for $VU_APP_BUNDLE_PATH"
codesign --verify --deep --strict --verbose=2 "$VU_APP_BUNDLE_PATH"

is_adhoc_signature=0
if codesign -dv "$VU_APP_BUNDLE_PATH" 2>&1 | grep -q 'Signature=adhoc'; then
  is_adhoc_signature=1
fi

if [[ "$is_adhoc_signature" == "1" ]]; then
  log "Skipping Gatekeeper verification for ad-hoc signature"
else
  require_cmd spctl
  spctl -a -vv --type exec "$VU_APP_BUNDLE_PATH"
fi

if [[ -f "$VU_DMG_PATH" ]]; then
  log "Verifying DMG signature for $VU_DMG_PATH"
  codesign --verify --verbose=2 "$VU_DMG_PATH"
fi

if [[ "$is_adhoc_signature" == "0" && "${VU_SKIP_NOTARIZATION:-0}" != "1" ]] && have_notary_credentials; then
  require_cmd xcrun
  xcrun stapler validate "$VU_APP_BUNDLE_PATH"
  xcrun stapler validate "$VU_DMG_PATH"
fi

log "Verification complete"
