#!/usr/bin/env bash
#
# Build a Linux release tarball for vu. Mirrors the role of
# `scripts/macos/release.sh` and `release-windows.yml`'s "Package
# ZIP" step:
#   - cargo build -p vu -p vu-cli --release with the channel + version baked
#     in via VU_RELEASE_CHANNEL / VU_RELEASE_VERSION (the
#     non-macOS updater path uses option_env!() to read these),
#   - stage `vu` + `vu-cli` + LICENSE + README.md + a `.desktop`
#     entry + a 256x256 icon into a versioned directory,
#   - emit `vu-<version>-linux-<arch>.tar.gz` plus a sha256 sum file
#     so the publish step can attach both to the GitHub release.
#
# Designed to be run both from CI (release-linux.yml) and locally
# (when verifying the artifact shape before tagging). Defaults to
# the host architecture; `VU_LINUX_ARCH=arm64` lets you label an
# aarch64 build the same way the macOS pipeline does.
#
# Output:
#   dist/vu-<version>-linux-<arch>/         staging dir
#   dist/vu-<version>-linux-<arch>.tar.gz   release artifact
#   dist/SHA256SUMS-linux.txt                checksum line for the publish step
#
# Required env (set by CI; can be overridden locally):
#   VU_RELEASE_VERSION   full tag-derived version, e.g. 0.1.0-beta.31
#   VU_RELEASE_CHANNEL   "stable" | "beta" | "dev"
#
# Optional:
#   VU_LINUX_ARCH        "x86_64" (default on x86_64 hosts) | "arm64"
#   CARGO_TARGET_DIR      forwarded to cargo if set

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

# ── Resolve metadata ────────────────────────────────────────────────────────

version="${VU_RELEASE_VERSION:-}"
if [[ -z "$version" ]]; then
  # Local invocation. Pull from the most recent annotated tag if we
  # can; otherwise fall back to the workspace package version.
  version="$(git describe --tags --abbrev=0 2>/dev/null | sed 's/^v//' || true)"
  if [[ -z "$version" ]]; then
    version="$(grep -E '^version' Cargo.toml | head -1 | cut -d'"' -f2)"
  fi
fi
[[ -n "$version" ]] || { echo "could not determine version" >&2; exit 1; }

channel="${VU_RELEASE_CHANNEL:-stable}"

host_arch="$(uname -m)"
case "${VU_LINUX_ARCH:-$host_arch}" in
  x86_64|amd64) art_arch="x86_64" ;;
  arm64|aarch64) art_arch="arm64" ;;
  *) echo "unsupported arch: ${VU_LINUX_ARCH:-$host_arch}" >&2; exit 1 ;;
esac

stage_name="vu-${version}-linux-${art_arch}"
stage_dir="dist/${stage_name}"
tarball="dist/${stage_name}.tar.gz"
sha_file="dist/SHA256SUMS-linux.txt"
linux_app_id="co.nowledge.vu"

echo "==> vu linux release"
echo "    version : ${version}"
echo "    channel : ${channel}"
echo "    arch    : ${art_arch}"
echo "    tarball : ${tarball}"
echo

# ── Build ───────────────────────────────────────────────────────────────────

# Match the Windows + macOS pipelines: bake version + channel into
# the binary at compile time so option_env!() in
# crates/vu-core/src/release_channel.rs picks them up. The Linux
# updater (added alongside this script) reads the channel to decide
# which Sparkle-shaped appcast to poll.
export VU_RELEASE_VERSION="${version}"
export VU_RELEASE_CHANNEL="${channel}"

echo "==> cargo build -p vu -p vu-cli --release"
cargo build -p vu -p vu-cli --release

# Resolve the actual target dir cargo wrote to. CARGO_TARGET_DIR
# overrides override the default.
target_dir="${CARGO_TARGET_DIR:-target}"
bin_path="${target_dir}/release/vu"
cli_bin_path="${target_dir}/release/vu-cli"
[[ -x "$bin_path" ]] || { echo "build did not produce $bin_path" >&2; exit 1; }
[[ -x "$cli_bin_path" ]] || { echo "build did not produce $cli_bin_path" >&2; exit 1; }

# ── Stage ───────────────────────────────────────────────────────────────────

echo "==> staging ${stage_dir}"
rm -rf "${stage_dir}"
mkdir -p "${stage_dir}"

# Strip debug info — drops the binary from ~300 MB to ~80 MB. The
# macOS pipeline does the equivalent via an Xcode strip phase; the
# Windows pipeline ships with debug-info stripped at link time.
cp "$bin_path" "${stage_dir}/vu"
strip --strip-debug "${stage_dir}/vu" || true
cp "$cli_bin_path" "${stage_dir}/vu-cli"
strip --strip-debug "${stage_dir}/vu-cli" || true

# Docs.
[[ -f LICENSE ]] && cp LICENSE "${stage_dir}/"
[[ -f README.md ]] && cp README.md "${stage_dir}/"

# Desktop entry. The filename, StartupWMClass, and runtime GPUI app_id must stay
# in sync so Wayland and X11 desktops group the running window with the launcher.
# The install.sh script rewrites the Exec line to
# point at the per-user install path before dropping it into
# ~/.local/share/applications, so the path here is just a default
# for users who hand-extract the tarball into /usr/local.
cat > "${stage_dir}/${linux_app_id}.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=vu
GenericName=Terminal
Comment=GPU-accelerated terminal emulator with a built-in AI agent harness
Exec=/usr/local/bin/vu %U
Icon=vu
Terminal=false
Categories=System;TerminalEmulator;Utility;
Keywords=terminal;shell;command;cli;ai;agent;
StartupWMClass=${linux_app_id}
EOF

# 256x256 icon — freedesktop hicolor's bread-and-butter size, and
# what GNOME / KDE / xfce / wayland launchers all hit-test against
# first. Pull from the macOS app-icon set we already ship.
icon_src="assets/Vu-macOS-Dark-256x256@2x.png"
if [[ -f "$icon_src" ]]; then
  cp "$icon_src" "${stage_dir}/vu.png"
else
  echo "warning: ${icon_src} missing — Linux tarball ships without an app icon" >&2
fi

# ── Tarball + checksum ──────────────────────────────────────────────────────

echo "==> packaging ${tarball}"
tar -C dist -czf "${tarball}" "${stage_name}"

echo "==> sha256"
sha256_out=""
if command -v sha256sum >/dev/null 2>&1; then
  sha256_out="$(cd dist && sha256sum "$(basename "${tarball}")")"
elif command -v shasum >/dev/null 2>&1; then
  sha256_out="$(cd dist && shasum -a 256 "$(basename "${tarball}")")"
else
  echo "warning: no sha256sum / shasum found — skipping checksum file" >&2
fi
if [[ -n "$sha256_out" ]]; then
  echo "${sha256_out}" > "${sha_file}"
  echo "    ${sha256_out}"
fi

echo
echo "==> done"
# `sha_file` is always non-empty (it's a default-derived path), so the
# previous `${sha_file:+"$sha_file"}` always expanded — `ls` then
# failed loudly when checksum generation was skipped because neither
# sha256sum nor shasum was on PATH. Test for the file's existence
# instead so the listing matches what was actually produced.
if [[ -f "$sha_file" ]]; then
  ls -la "${tarball}" "${sha_file}"
else
  ls -la "${tarball}"
fi
