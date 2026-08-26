# vu architecture

vu is an open-source GPU-accelerated terminal emulator written in Rust. It keeps terminal rendering, application chrome, saved workspaces, and the local control API in separate crates.

## Platform backends

| Platform | Terminal backend | Window rendering |
| --- | --- | --- |
| macOS | embedded libghostty | Metal |
| Windows | ConPTY with libghostty-vt | D3D11 and DirectWrite |
| Linux | Unix PTY with libghostty-vt | GPUI styled text |

The app uses the same `GhosttyApp`, `GhosttyTerminal`, and `TerminalColors` names on every platform. Platform-specific code stays inside `vu-ghostty`.

## Platform strategy

vu now has three platform states:

- macOS: shipped, using the embedded libghostty + AppKit path
- Windows: beta, using `libghostty-vt` + ConPTY + a local D3D11/DirectWrite renderer
- Linux: preview, using Unix PTY + shared `libghostty-vt` + a GPUI-owned per-row `StyledText` paint path. SGR colors / bold / italic / underline / inverse + block cursor all work, the window ships with client-side decorations, transparent ARGB visual, rounded corners, and KWin-Wayland backdrop blur where the compositor exposes it. The long-term GPUI-owned glyph-atlas grid renderer (matching the D3D11/DirectWrite path Windows uses) is the remaining Linux work.

The platform architecture is no longer "macOS only," but it is still not
"one backend everywhere."

- GPUI provides the host app shell on all supported platforms
- Ghostty remains the preferred terminal truth source whenever its
  embedding surface is available
- platform-specific backend glue still matters:
  - AppKit on macOS
  - D3D11/ConPTY on Windows today
  - Unix PTY + GPUI-owned renderer on Linux; see `docs/impl/linux-port.md`

Important consequence:

- Windows proved that vu can ship a local backend when upstream
  embedding is unavailable.
- Linux feasibility work showed that upstream Ghostty and GPUI both have
  real Linux stacks, but their embedding boundaries do not line up for
  vu today.
- vu therefore takes the same delivery stance on Linux that it used on
  Windows: ship a local backend instead of waiting on upstream embed
  hooks.

## Workspace layout

```text
crates/
├── vu-app/       GPUI application, windows, tabs, panes, settings
├── vu-core/      config, session persistence, workspace layouts, control API
├── vu-paths/     platform-specific filesystem paths
├── vu-terminal/  themes and palette helpers
├── vu-ghostty/   terminal backends and PTY integration
├── vu-cli/       client for the local control socket
└── vu-test/      integration runner
```

Crate boundaries are deliberate. `vu-terminal` has no UI dependencies. `vu-core` owns serializable state and protocol types. `vu-app` connects GPUI entities to terminal backends.

## Terminal model

A window owns tabs. Each tab owns a pane tree. A pane can contain multiple terminal surfaces, with one active surface at a time.

```text
window
└── tab
    └── pane tree
        ├── pane
        │   ├── active terminal surface
        │   └── inactive terminal surfaces
        └── pane
```

Ghostty remains the source of truth for terminal content, cursor state, titles, working directories, and process lifecycle where the backend exposes them. Vu records only the state needed for layout, restoration, and control.

## Runtime observation

Pane runtime state combines backend facts, shell integration, visible-screen hints, and commands issued through Vu. Each fact carries its source and confidence. Historical commands can explain how a pane reached its current state, but they do not override newer terminal evidence.

The control layer distinguishes the outer Vu pane from nested targets such as tmux panes. A pane index always addresses the Vu pane. Native tmux commands use tmux target identifiers.

## Local control API

`vu-core` implements a JSON-RPC 2.0 protocol. The app serves it over a Unix domain socket on macOS and Linux or a named pipe on Windows. `vu-cli` is the first client.

Supported method groups cover:

- application identity and capabilities
- tabs and pane trees
- pane reads, writes, creation, and readiness
- pane-local terminal surfaces
- tmux inspection and commands

Commands that create or drive terminals execute through the visible PTY. The user sees the resulting terminal activity.

## Persistence

Private session state is stored in the platform app-data directory. Project layout profiles use `.vu/workspace.toml` and contain layout intent only: tabs, panes, surfaces, split geometry, labels, and working directories. Exported profiles exclude scrollback, command history, credentials, and other private runtime state.

## UI structure

GPUI owns application chrome, tabs, sidebars, settings, command palette, and editor views. Embedded terminal views remain native on macOS. Windows and Linux terminal views use their platform renderer behind the same workspace-facing API.

The visual system uses opacity-based surfaces, no decorative shadows, Phosphor icons, Ioskeley Mono for terminal chrome, and the system UI font for settings.

## Build and release

Rust stable is the application toolchain. Zig 0.15.2 is pinned for libghostty and libghostty-vt builds. The root `justfile` contains local build, test, packaging, and install commands. Platform release scripts create the distributable artifacts and run platform-specific verification.

### Build pipeline

1. Cargo resolves workspace deps from upstream git sources and crates.io as declared in the workspace manifest
2. GPUI compiles Metal shaders at runtime (`runtime_shaders` feature — no Xcode.app needed for dev)
3. `cargo build` produces the `vu` binary with all crates linked

## Config

```toml
# ~/.config/vu/config.toml

[terminal]
font-family = "JetBrains Mono"
font-size = 14
theme = "catppuccin-mocha"        # or any ghostty theme
scrollback-lines = 10000
cursor-style = "block"

[keybindings]
command-palette = "cmd+shift+p"
new-tab = "cmd+t"
```

## Resolved decisions

### 1. Terminal Backend: Full Ghostty

**Decision:** Embed full Ghostty via libghostty C API as the only terminal runtime.

**Rationale:**

- Ghostty provides production-grade VT compliance (Kitty keyboard, sixel, hyperlinks, OSC 133) without reimplementing it
- GPU-accelerated Metal rendering via native NSView — superior performance to software rasterization
- The `vu-ghostty` crate is a thin FFI wrapper (~800 lines), not a fork — upstream Ghostty updates flow through cleanly
- `TerminalPane` remains as a stable pane-facing API for the workspace layer

### 2. GPUI IME: Production-Ready

GPUI implements the full `InputHandler` trait (modeled after `NSTextInputClient`):

- `marked_text_range()` / `replace_and_mark_text_in_range()` for IME composition
- `bounds_for_range()` for candidate window positioning
- GPUI has broader platform support, and vu now ships macOS plus a
  Windows beta and a Linux preview; see `docs/impl/linux-port.md`.
- CJK input works. No blocker.

### 3. GPU Fallback: Not Needed

Ghostty has no software renderer — GPU-only. Same for vu.

**Rationale:** A desktop terminal emulator always has a GPU. The scenarios where it doesn't:

- **Headless servers** — users SSH in, they use the remote machine's terminal, not vu
- **Containers** — vu doesn't run inside Docker; it runs on the host

If we ever need headless testing, we add a test-only software rasterizer. Not a user feature.

### 4. Plugin System: Node.js + Python via Sidecar IPC

**Decision:** Plugins run as external processes. vu communicates via JSON-RPC over Unix domain sockets.

**Why Node.js + Python:**

- Largest developer ecosystems — lowest friction for plugin authors
- Runtimes installed by the user (system Node/Python, nvm, pyenv — their choice)
- No embedded runtime = no binary bloat, no version conflicts
- Security: plugins run in separate processes with explicit capability grants

**How it works:**

```
vu (Rust)  ─── Unix socket (JSON-RPC) ───  plugin process (Node/Python/any)
```

- vu exposes a socket API: `notification.create`, `terminal.write`, `context.get`, etc.
- Plugin manifest declares: name, runtime, entry point, requested capabilities
- vu spawns the plugin process, passes socket path via env var
- Plugin SDK: thin npm package (`@vu/sdk`) and pip package (`vu-sdk`) wrapping the JSON-RPC protocol

**Phase 4 deliverable.** Socket API comes from the same external automation work introduced earlier in the product plan.

### 5. Licensing: MIT

- **GPUI**: Apache 2.0 (compatible with MIT, allows sublicensing)
- **Ghostty libghostty**: MIT
- **vu**: MIT (already in LICENSE)

All clear. No copyleft, no GPL contamination.
