# vu — justfile
# https://github.com/casey/just
#
# Usage:
#   just          # list all recipes
#   just run      # run from source (current platform)
#   just install  # build and install (current platform)
#
# The `arch` parameter defaults to "" — each Unix recipe auto-detects via
# `uname -m` inside the shell body. Windows recipes never reference arch so
# `uname` is never invoked there.
# Override explicitly when needed: just arch=x86_64 macos-bundle

# Use cmd.exe on Windows so recipes work in a plain Developer Command Prompt
# without requiring Git Bash, Cygwin, or sh on PATH.
set windows-shell := ["cmd.exe", "/c"]

# ── defaults ──────────────────────────────────────────────────────────────────

# Release channel for macOS/Linux app bundles (stable | beta | dev)
channel := "stable"

# Target architecture. Empty = auto-detect inside each recipe (Unix only).
# Windows recipes never use this variable so uname is never called there.
arch := ""

# ── list ──────────────────────────────────────────────────────────────────────

# List all recipes (default)
default:
    @just --list

# ── universal dev commands ────────────────────────────────────────────────────
# These dispatch to the right platform recipe. Windows must use the `w*` cargo
# aliases because Windows release workflows retain the feature-gated
# `vu-app.exe` target.

# Debug build — current platform
build:
    {{ if os() == "windows" { "cargo wbuild -p vu" } else { "cargo build -p vu" } }}

# Release build — current platform
build-release:
    {{ if os() == "windows" { "cargo wbuild -p vu --release" } else { "cargo build --release -p vu" } }}

# Run from source — current platform
run:
    {{ if os() == "windows" { "cargo wrun -p vu" } else { "cargo run -p vu" } }}

# Run the platform-appropriate test set
test:
    {{ if os() == "windows" { "cargo wtest -p vu-core -p vu-cli -p vu-terminal" } else { "cargo test --workspace" } }}

# Check without building — current platform
check:
    {{ if os() == "windows" { "cargo wcheck -p vu" } else { "cargo check --workspace" } }}

# Run clippy — current platform
lint:
    {{ if os() == "windows" { "cargo clippy --workspace --no-default-features --features vu/bin-vu-app -- -D warnings" } else { "cargo clippy --workspace -- -D warnings" } }}

# Clean cargo build artifacts
clean:
    cargo clean

# Build and install to the current platform's local development install path
install:
    just channel={{ channel }} arch={{ arch }} {{ if os() == "macos" { "macos-install" } else if os() == "linux" { "linux-install" } else if os() == "windows" { "windows-install" } else { "unsupported-platform" } }}

# Print the current package id, including the workspace version
version:
    @cargo pkgid -p vu

unsupported-platform:
    @echo "Unsupported platform for this justfile"
    @exit 1

# ── macOS ─────────────────────────────────────────────────────────────────────

# [macOS] Build and ad-hoc sign a local .app bundle
# Output: dist/macos/{channel}/{arch}/vu.app
macos-bundle channel=channel arch=arch:
    #!/usr/bin/env bash
    set -euo pipefail
    resolved_arch="{{ arch }}"
    if [[ -z "${resolved_arch}" ]]; then
        resolved_arch="$(uname -m | sed 's/aarch64/arm64/')"
    fi
    VU_CHANNEL={{ channel }} VU_ARCH="${resolved_arch}" ./scripts/macos/build-app.sh
    app_name="vu"
    if [[ "{{ channel }}" == "beta" ]]; then app_name="vu Beta"; fi
    if [[ "{{ channel }}" == "dev" ]];  then app_name="vu Dev";  fi
    bundle="dist/macos/{{ channel }}/${resolved_arch}/${app_name}.app"
    codesign --force --deep --sign - "${bundle}"

# [macOS] Build .app and copy to /Applications (replaces existing)
macos-install channel=channel arch=arch: (macos-bundle channel arch)
    #!/usr/bin/env bash
    set -euo pipefail
    resolved_arch="{{ arch }}"
    if [[ -z "${resolved_arch}" ]]; then
        resolved_arch="$(uname -m | sed 's/aarch64/arm64/')"
    fi
    app_name="vu"
    if [[ "{{ channel }}" == "beta" ]]; then app_name="vu Beta"; fi
    if [[ "{{ channel }}" == "dev" ]];  then app_name="vu Dev";  fi
    src="dist/macos/{{ channel }}/${resolved_arch}/${app_name}.app"
    dst="/Applications/${app_name}.app"
    echo "Installing ${src} → ${dst}"
    rm -rf "${dst}"
    cp -R "${src}" "${dst}"
    echo "Done. Launch ${app_name} from /Applications or Spotlight."

# [macOS] Full release: build + sign + notarize + DMG
# Requires: APPLE_SIGNING_IDENTITY + APPLE_NOTARY_* or APPLE_ID env vars
macos-release channel=channel arch=arch:
    #!/usr/bin/env bash
    set -euo pipefail
    resolved_arch="{{ arch }}"
    if [[ -z "${resolved_arch}" ]]; then
        resolved_arch="$(uname -m | sed 's/aarch64/arm64/')"
    fi
    VU_CHANNEL={{ channel }} VU_ARCH="${resolved_arch}" ./scripts/macos/release.sh

# [macOS] Download Sparkle.framework into .sparkle/ (enables auto-update in bundle)
macos-sparkle-download:
    ./scripts/sparkle/download.sh

# [macOS] Open the built app bundle in Finder
macos-open channel=channel arch=arch:
    #!/usr/bin/env bash
    resolved_arch="{{ arch }}"
    if [[ -z "${resolved_arch}" ]]; then
        resolved_arch="$(uname -m | sed 's/aarch64/arm64/')"
    fi
    app_name="vu"
    if [[ "{{ channel }}" == "beta" ]]; then app_name="vu Beta"; fi
    if [[ "{{ channel }}" == "dev" ]];  then app_name="vu Dev";  fi
    open "dist/macos/{{ channel }}/${resolved_arch}/${app_name}.app"

# ── Linux ─────────────────────────────────────────────────────────────────────

# [Linux] Build a release binary and package it
# Output: dist/vu-{version}-linux-{arch}.tar.gz
linux-release channel=channel arch=arch:
    #!/usr/bin/env bash
    set -euo pipefail
    resolved_arch="{{ arch }}"
    if [[ -z "${resolved_arch}" ]]; then
        resolved_arch="$(uname -m | sed 's/aarch64/arm64/')"
    fi
    VU_RELEASE_CHANNEL={{ channel }} VU_LINUX_ARCH="${resolved_arch}" ./scripts/linux/release.sh

# [Linux] Install the release binaries to ~/.local/bin
linux-install channel=channel arch=arch: (linux-release channel arch)
    #!/usr/bin/env bash
    set -euo pipefail
    resolved_arch="{{ arch }}"
    if [[ -z "${resolved_arch}" ]]; then
        resolved_arch="$(uname -m | sed 's/aarch64/arm64/')"
    fi
    # scripts/linux/release.sh stages to dist/vu-{version}-linux-{arch}/
    # Use || true so set -e doesn't exit when the glob has no matches.
    stage_dir="$(ls -d dist/vu-*-linux-${resolved_arch} 2>/dev/null | sort -V | tail -1 || true)"
    if [[ -z "${stage_dir}" || ! -f "${stage_dir}/vu" ]]; then
        echo "Binary not found under dist/vu-*-linux-${resolved_arch}/ — run 'just linux-release' first"
        exit 1
    fi
    mkdir -p "$HOME/.local/bin"
    cp "${stage_dir}/vu" "$HOME/.local/bin/vu"
    chmod 755 "$HOME/.local/bin/vu"
    echo "Installed ${stage_dir}/vu → $HOME/.local/bin/vu"
    if [[ -f "${stage_dir}/vu-cli" ]]; then
        cp "${stage_dir}/vu-cli" "$HOME/.local/bin/vu-cli"
        chmod 755 "$HOME/.local/bin/vu-cli"
        echo "Installed ${stage_dir}/vu-cli → $HOME/.local/bin/vu-cli"
    fi

# ── Windows (run from Developer Command Prompt for VS 2022) ───────────────────

# [Windows] Debug build (`vu-app.exe` release alias)
windows-build:
    cargo wbuild -p vu

# [Windows] Release build
windows-build-release:
    cargo wbuild -p vu --release
    cargo build -p vu-cli --release

# [Windows] Run
windows-run:
    cargo wrun -p vu

# [Windows] Test
windows-test:
    cargo wtest -p vu-core -p vu-cli -p vu-terminal

# [Windows] Build and install local release binaries to the user install root
windows-install: windows-build-release
    if not exist "%LOCALAPPDATA%\Programs\vu" mkdir "%LOCALAPPDATA%\Programs\vu"
    copy /Y "target\release\vu-app.exe" "%LOCALAPPDATA%\Programs\vu\vu-app.exe"
    copy /Y "target\release\vu-cli.exe" "%LOCALAPPDATA%\Programs\vu\vu-cli.exe"
    echo Installed vu-app.exe and vu-cli.exe to %LOCALAPPDATA%\Programs\vu

# ── dist cleanup ──────────────────────────────────────────────────────────────

# Remove all dist/ output
clean-dist:
    {{ if os() == "windows" { "if exist dist rmdir /s /q dist" } else { "rm -rf dist/" } }}
