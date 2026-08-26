# Implementation: Build System

## Overview

vu is a Cargo workspace with platform terminal backends:

- macOS embeds full Ghostty with Metal rendering.
- Windows uses ConPTY plus `libghostty-vt` and a D3D11/DirectWrite renderer.
- Linux uses a Unix PTY plus `libghostty-vt` and the preview GPUI paint path.

The build no longer includes the old `vte` and `portable-pty` pipeline.

## Build commands

```bash
cargo build
cargo build --release
cargo run -p vu
cargo test --workspace
```

## Workspace shape

```toml
[workspace]
members = [
    "crates/vu-app",
    "crates/vu-core",
    "crates/vu-paths",
    "crates/vu-terminal",
    "crates/vu-ghostty",
    "crates/vu-cli",
]
```

## Key crates

| Crate | Purpose |
|-------|---------|
| `vu` | GPUI app shell, tabs, splits, and settings |
| `vu-core` | config and session persistence |
| `vu-paths` | shared platform-safe per-user app directories |
| `vu-ghostty` | Rust wrapper around libghostty C API |
| `vu-terminal` | terminal theme data and Ghostty palette translation helpers |

## Key dependencies

| Dependency | Purpose |
|------------|---------|
| `gpui` | native GPU UI framework (upstream Zed git source) |
| `gpui-component` | reusable UI controls (upstream Longbridge git source) |
| `crossbeam-channel` | UI event routing |

## Dependency sourcing

`vu` does not build against live sources in `3pp/`.

- `3pp/` is read-only reference material only.
- Cargo dependencies resolve from crates.io or explicit git sources in the workspace manifest.
- Ghostty source is fetched by `vu-ghostty/build.rs` when needed, unless an override source directory is provided for local development.

## Platform boundary

The `vu` UI binary builds on macOS, Windows, and Linux. Platform-specific
runtime code is `cfg`-gated:

- macOS uses the embedded libghostty surface.
- Windows release workflows retain the `vu-app.exe` binary alias.
- Linux keeps the normal `vu` binary name.

## Per-user app paths

Use `vu-paths` for all user config, data, auth, theme, and skill storage paths.
Windows uses the `vu-terminal` app-directory name, so `vu-paths` maps the app
directory to `vu-terminal` on Windows while preserving `vu` on macOS and Linux.

## Ghostty build boundary

`vu-ghostty` is intentionally thin:

- FFI bindings live in `ffi.rs`
- surface/app lifecycle lives in `terminal.rs`
- product logic stays out of the wrapper

## Ghostty resources

Ghostty's runtime is not just the static library. The child shell environment also depends on the bundled Ghostty resources payload, especially:

- `terminfo/xterm-ghostty`
- shell integration scripts
- supporting share files under `Resources/ghostty`

Vu now handles this in two places:

- `cargo run -p vu` debug runs: `vu-ghostty` seeds `GHOSTTY_RESOURCES_DIR` from the built Ghostty `zig-out/share/ghostty` directory when that directory exists locally.
- macOS app bundles: `scripts/macos/build-app.sh` copies Ghostty's built `share/ghostty` tree into `Contents/Resources/ghostty`.

Without that payload, Ghostty falls back to `TERM=xterm-256color` and disables parts of shell integration. That changes the behavior child processes see and can invalidate product comparisons against standalone Ghostty.

If we need stronger pane observability in the future, the preferred path is to upstream or expose new libghostty C API surface area instead of growing another terminal runtime in this repo.
