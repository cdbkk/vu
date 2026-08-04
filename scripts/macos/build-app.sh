#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/macos/common.sh
source "$SCRIPT_DIR/common.sh"

setup_release_env

require_cmd cargo
require_cmd iconutil
require_cmd sips
require_cmd rsync
require_cmd mkdir

mkdir -p "$VU_DIST_ROOT"

log "Building vu and vu-cli for $VU_RUST_TARGET"
(
  cd "$REPO_ROOT"
  VU_REQUIRE_GHOSTTY_INITIAL_OUTPUT="${VU_REQUIRE_GHOSTTY_INITIAL_OUTPUT:-1}" \
    cargo build --locked --release --target "$VU_RUST_TARGET" -p vu -p vu-cli
)

app_root="$VU_APP_BUNDLE_PATH"
contents_dir="$app_root/Contents"
macos_dir="$contents_dir/MacOS"
resources_dir="$contents_dir/Resources"
binary_path="$REPO_ROOT/target/$VU_RUST_TARGET/release/vu"
cli_binary_path="$REPO_ROOT/target/$VU_RUST_TARGET/release/vu-cli"

rm -rf "$app_root"
mkdir -p "$macos_dir" "$resources_dir"

log "Creating app bundle at $app_root"
rsync -a "$binary_path" "$macos_dir/vu"
chmod 755 "$macos_dir/vu"
rsync -a "$cli_binary_path" "$macos_dir/vu-cli"
chmod 755 "$macos_dir/vu-cli"

ghostty_resources_dir="$(find "$REPO_ROOT/target/$VU_RUST_TARGET/release/build" -path '*/out/ghostty-src/zig-out/share/ghostty' | head -n 1)"
if [[ -z "$ghostty_resources_dir" || ! -d "$ghostty_resources_dir" ]]; then
  log "Ghostty resources not found in cargo build output"
  exit 1
fi
rsync -a "$ghostty_resources_dir/" "$resources_dir/ghostty/"
log "Embedded Ghostty resources from $ghostty_resources_dir"

ghostty_share_dir="$(dirname "$ghostty_resources_dir")"
ghostty_terminfo_dir="$ghostty_share_dir/terminfo"
if [[ ! -d "$ghostty_terminfo_dir" ]]; then
  fail "Ghostty terminfo directory not found in cargo build output: $ghostty_terminfo_dir"
fi
ghostty_terminfo_entry="$(find "$ghostty_terminfo_dir" -type f -name xterm-ghostty -print -quit)"
if [[ -z "$ghostty_terminfo_entry" || ! -r "$ghostty_terminfo_entry" ]]; then
  fail "Ghostty xterm-ghostty terminfo entry not found under cargo build output: $ghostty_terminfo_dir"
fi
rsync -a "$ghostty_terminfo_dir/" "$resources_dir/terminfo/"
log "Embedded Ghostty terminfo from $ghostty_terminfo_dir"

iconset_parent="$(mktemp -d "$VU_DIST_ROOT/iconset.XXXXXX")"
iconset_dir="$iconset_parent/vu.iconset"
mkdir -p "$iconset_dir"
trap 'rm -rf "$iconset_parent"' EXIT

# The 16pt and 32pt slots get their own artwork. Downscaling the full mark
# to that size turns it into a dark blob: the two letterforms crowd the
# tile's curved edges and the strokes fall below a pixel. The small master
# is a single `v` monogram on an apricot tile instead.
icon_source_small="${VU_ICON_SOURCE_SMALL:-$REPO_ROOT/assets/Vu-macOS-Dark-Small-1024x1024@1x.png}"
if [[ ! -r "$icon_source_small" ]]; then
  icon_source_small="$VU_ICON_SOURCE"
fi

for size in 128 256 512; do
  sips -z "$size" "$size" "$VU_ICON_SOURCE" --out "$iconset_dir/icon_${size}x${size}.png" >/dev/null
done

for size in 16 32; do
  sips -z "$size" "$size" "$icon_source_small" --out "$iconset_dir/icon_${size}x${size}.png" >/dev/null
done

sips -z 32 32 "$icon_source_small" --out "$iconset_dir/icon_16x16@2x.png" >/dev/null
sips -z 64 64 "$icon_source_small" --out "$iconset_dir/icon_32x32@2x.png" >/dev/null
sips -z 256 256 "$VU_ICON_SOURCE" --out "$iconset_dir/icon_128x128@2x.png" >/dev/null
sips -z 512 512 "$VU_ICON_SOURCE" --out "$iconset_dir/icon_256x256@2x.png" >/dev/null
cp "$VU_ICON_SOURCE" "$iconset_dir/icon_512x512@2x.png"

iconutil -c icns "$iconset_dir" -o "$resources_dir/vu.icns"
generate_info_plist "$contents_dir/Info.plist"

printf 'APPL????' >"$contents_dir/PkgInfo"

# Embed Sparkle.framework if available (downloaded by scripts/sparkle/download.sh)
sparkle_framework="${SPARKLE_DIR:-$REPO_ROOT/.sparkle}/Sparkle.framework"
if [[ -d "$sparkle_framework" ]]; then
  frameworks_dir="$contents_dir/Frameworks"
  mkdir -p "$frameworks_dir"
  rsync -a "$sparkle_framework" "$frameworks_dir/"
  log "Embedded Sparkle.framework"
else
  log "Sparkle.framework not found — auto-update will be disabled at runtime"
fi

log "App bundle ready: $VU_APP_BUNDLE_PATH"
