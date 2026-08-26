#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/macos/common.sh
source "$SCRIPT_DIR/common.sh"

setup_release_env

require_cmd codesign
require_cmd ditto
require_cmd hdiutil
require_cmd rsync
require_cmd shasum

sign_identity_value="$(signing_identity)"
if [[ "$sign_identity_value" == "-" && "${VU_SKIP_NOTARIZATION:-0}" != "1" ]] && have_notary_credentials; then
  fail "notarization requires APPLE_SIGNING_IDENTITY"
fi

"$SCRIPT_DIR/build-app.sh"

run_codesign() {
  local max_attempts=4
  local delay=10
  local attempt

  for attempt in $(seq 1 "$max_attempts"); do
    local status=0
    if codesign "$@"; then
      return 0
    else
      status=$?
    fi

    if [[ "$attempt" -ge "$max_attempts" ]]; then
      return "$status"
    fi

    log "codesign failed with exit status $status; retrying in ${delay}s (${attempt}/${max_attempts})"
    sleep "$delay"
    delay=$((delay * 2))
  done
}

sign_code() {
  local path="$1"
  log "Signing $path"
  run_codesign --force --sign "$sign_identity_value" --timestamp --options runtime "$path"
}

sign_container() {
  local path="$1"
  log "Signing $path"
  run_codesign --force --sign "$sign_identity_value" --timestamp "$path"
}

sign_app_bundle() {
  local app_path="$1"

  # Signing must proceed inside-out: deepest nested code first,
  # then enclosing bundles, then the app itself.
  #
  # Strategy: collect every signable target with its depth, sort
  # deepest-first, sign in that order.  This avoids recursive
  # functions and the double-signing they risk.

  local frameworks_dir="$app_path/Contents/Frameworks"

  # 1. Sign all individual executables and libraries inside embedded
  #    frameworks (XPC helpers, nested dylibs, etc.).
  if [[ -d "$frameworks_dir" ]]; then
    while IFS= read -r nested; do
      sign_code "$nested"
    done < <(
      find "$frameworks_dir" -type f \
        \( -name '*.dylib' -o -name '*.so' -o -perm -111 \) \
        ! -path '*/Resources/*' \
        | sort
    )

    # 2. Sign XPC service bundles (inside frameworks).
    while IFS= read -r -d '' xpc; do
      sign_code "$xpc"
    done < <(find "$frameworks_dir" -name '*.xpc' -print0 2>/dev/null || true)

    # 3. Sign framework bundles themselves.
    while IFS= read -r -d '' fw; do
      sign_code "$fw"
    done < <(find "$frameworks_dir" -maxdepth 1 -name '*.framework' -print0 2>/dev/null || true)
  fi

  # 4. Sign loose executables in the app, excluding anything already covered
  #    by the framework pass. Sign auxiliary executables before the main app
  #    binary: codesign validates nested/sibling executable code when signing
  #    the bundle's main executable, so vu-cli must already be signed.
  local main_executable="$app_path/Contents/MacOS/vu"
  local has_main_executable=0
  while IFS= read -r nested; do
    if [[ "$nested" == "$main_executable" ]]; then
      has_main_executable=1
      continue
    fi
    sign_code "$nested"
  done < <(
    find "$app_path/Contents" -type f \
      \( -name '*.dylib' -o -name '*.so' -o -perm -111 \) \
      ! -path '*/Resources/*' \
      ! -path '*/Frameworks/*' \
      | sort
  )

  if [[ "$has_main_executable" == "1" ]]; then
    sign_code "$main_executable"
  fi

  # 5. Sign the top-level app bundle.
  sign_code "$app_path"
}

package_dmg() {
  local staging_dir
  staging_dir="$(mktemp -d "$VU_DIST_ROOT/dmg.XXXXXX")"

  rsync -a "$VU_APP_BUNDLE_PATH" "$staging_dir/"
  ln -s /Applications "$staging_dir/Applications"

  rm -f "$VU_DMG_PATH"
  hdiutil create \
    -volname "$VU_APP_NAME" \
    -srcfolder "$staging_dir" \
    -fs HFS+ \
    -format UDZO \
    -ov \
    "$VU_DMG_PATH"

  rm -rf "$staging_dir"
}

mkdir -p "$VU_DIST_ROOT"
rm -f "$VU_APP_ZIP_PATH" "$VU_DMG_PATH" "$VU_CHECKSUM_PATH"

sign_app_bundle "$VU_APP_BUNDLE_PATH"

ditto -c -k --keepParent "$VU_APP_BUNDLE_PATH" "$VU_APP_ZIP_PATH"
notarytool_submit "$VU_APP_ZIP_PATH"

if [[ "${VU_SKIP_NOTARIZATION:-0}" != "1" ]] && have_notary_credentials; then
  log "Stapling app bundle"
  xcrun stapler staple -v "$VU_APP_BUNDLE_PATH"
fi

package_dmg
sign_container "$VU_DMG_PATH"
notarytool_submit "$VU_DMG_PATH"

if [[ "${VU_SKIP_NOTARIZATION:-0}" != "1" ]] && have_notary_credentials; then
  log "Stapling dmg"
  xcrun stapler staple -v "$VU_DMG_PATH"
fi

"$SCRIPT_DIR/verify.sh"

(
  cd "$VU_DIST_ROOT"
  shasum -a 256 "$(basename "$VU_APP_ZIP_PATH")" "$(basename "$VU_DMG_PATH")" >"$(basename "$VU_CHECKSUM_PATH")"
)

log "Release artifacts:"
log "  app: $VU_APP_BUNDLE_PATH"
log "  zip: $VU_APP_ZIP_PATH"
log "  dmg: $VU_DMG_PATH"
log "  sha256: $VU_CHECKSUM_PATH"
