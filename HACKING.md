# Hacking on vu

Quick contributor map for `vu`.

Read these first:
- `README.md` — public project overview
- `CLAUDE.md` — development conventions
- `DESIGN.md` — architecture and product direction
- `docs/README.md` — documentation index

## Workspace Map

Terminal crates do not depend on the UI.

| Crate | Role |
|-------|------|
| `vu` | GPUI shell: windows, tabs, panes, settings, command surfaces |
| `vu-core` | Config, sessions, shared app logic, and control-plane types |
| `vu-terminal` | Theme + palette helpers shared across backends |
| `vu-ghostty` | Per-platform terminal backends: macOS embedded libghostty + Metal, Windows libghostty-vt + ConPTY + D3D11/DirectWrite, Linux Unix PTY + libghostty-vt + GPUI-owned `StyledText` paint |
| `vu-cli` | CLI + socket client for the live local control plane |

Each platform exposes the same `GhosttyApp` / `GhosttyTerminal` /
`TerminalColors` type names from `vu-ghostty`, so the rest of the
workspace consumes the backend without per-call-site `cfg` gates.
See `docs/impl/{linux,windows}-port.md` for the per-platform plans
and the path to the long-term GPU-accelerated grid renderer.

## Prerequisites

- Rust (stable, edition 2024)
- `cmake`
- **Zig**: use **Zig 0.15.2 exactly** for full terminal builds.
  Do not read this as `0.15.2+`: Zig `0.16.0` changes build APIs
  that the pinned Ghostty revision does not support yet, and
  `vu-ghostty` will fail while compiling libghostty.

### Quick setup with mise (recommended)

If you use [mise](https://mise.jdx.dev/), the repo root `mise.toml`
pins the exact Zig version. Run:

```bash
mise install   # installs zig 0.15.2
```

Then use `just` for all build / run / test tasks (see below).

### Manual setup

If you do not use mise, install the prerequisites yourself:

- **Zig**: download the official 0.15.2 archive from
  `https://ziglang.org/download/0.15.2/` and put the directory on
  `PATH`, or set `VU_ZIG_BIN=/path/to/zig`.
- **macOS**: `cmake` plus Zig 0.15.2. The macOS release workflow installs Zig 0.15.2 explicitly before building embedded libghostty.
- **Windows**: Zig 0.15.2, Visual Studio 2022 Build Tools with the Windows 10/11 SDK. Run full builds from a _Developer Command Prompt for VS 2022_ so `rc.exe` is on `PATH`. If Windows Defender is on, either add an exclusion for the repo dir or disable real-time scanning — Zig's sub-build exes get briefly locked by MpEngine and spawn with `FileNotFound`.
- **Linux**: Zig 0.15.2, plus the GPUI runtime apt deps the CI job already installs:
  ```sh
  sudo apt-get install -y --no-install-recommends \
    libxcb-composite0-dev libxcb-dri2-0-dev libxcb-glx0-dev \
    libxcb-present-dev libxcb-xfixes0-dev libxkbcommon-x11-dev \
    libwayland-dev libvulkan-dev libfreetype-dev libfontconfig1-dev \
    mesa-vulkan-drivers
  ```
  The `mesa-vulkan-drivers` line gives you a software ICD (llvmpipe) as a fallback for headless / VM environments; on a real desktop with a hardware GPU you can skip it.

CI mirrors this deliberately:
- `release-macos.yml`, `release-linux.yml`, and `release-windows.yml` install Zig 0.15.2 before release builds.
- The Linux PR smoke check in `ci-portable.yml` also installs Zig 0.15.2 because it type-checks `vu-ghostty` with `libghostty-vt`.
- The Windows PR smoke check sets `VU_SKIP_GHOSTTY_VT=1` because `cargo check` does not link and GitHub's Windows image does not ship our required Zig. That keeps PR checks fast, but it is not a substitute for a full Windows release build.

## Build

```bash
# macOS / Linux
cargo build

# Windows release workflows use the retained `vu-app.exe` alias.
# `cargo wbuild` is `cargo build --no-default-features
# --features vu/bin-vu-app`; `wrun`, `wcheck`, `wtest` mirror it.
cargo wbuild -p vu --release          # → target\release\vu-app.exe
```

If you have `just` installed, the root `justfile` wraps the common local
flows:

```bash
just build          # debug build for the current platform
just run            # run from source
just test           # platform-appropriate test set
just check          # fast type check
just install        # build and install to the local platform install path
```

On Windows, those default recipes dispatch through the `cargo w*` aliases
above, so they produce and run the Windows release alias `vu-app.exe`.
Platform-specific release helpers are also available,
for example `just channel=beta macos-release`,
`just channel=beta linux-release`, `just arch=x86_64 macos-bundle`, and
`just windows-build-release`.

## Run

```bash
# macOS / Linux
cargo run -p vu

# Windows
cargo wrun -p vu
```

## Test

```bash
cargo test --workspace            # macOS / Linux
cargo wtest -p vu-core -p vu-cli -p vu-terminal   # Windows (portable crates only)
```

## Release Build

```bash
cargo build --release -p vu                  # macOS / Linux
cargo build --release -p vu-cli              # control-plane CLI
cargo wbuild -p vu --release                 # Windows → target\release\vu-app.exe
cargo build --release -p vu-cli              # Windows → target\release\vu-cli.exe
```

For signed macOS release artifacts, use:

```bash
./scripts/macos/release.sh
```

The macOS app bundle contains both `Contents/MacOS/vu` and
`Contents/MacOS/vu-cli`; the release verifier fails if the CLI is
missing. The Homebrew cask and Unix installer expose that bundled
`vu-cli` on PATH so control clients do not need a separate source checkout.

Release CI also has a final promotion gate. Platform jobs verify the
artifact shape before upload, and `release-finalize.yml` keeps the
GitHub Release drafted unless all expected assets, appcasts, and
gh-pages installer scripts are present for the same tag. A broken
artifact should fail private, not become `/releases/latest`. Internal
`v*-dev.*` smoke tags are prereleases, never update public
stable/beta appcasts or Homebrew casks, do not embed a Sparkle feed URL,
and are only gated on artifact and installer-script shape.

For a Linux release tarball (un-signed; mirrors the Windows
preview's distribution shape), use:

```bash
VU_RELEASE_VERSION=0.1.0-beta.X VU_RELEASE_CHANNEL=beta \
  ./scripts/linux/release.sh
```

Output lands in `dist/vu-<version>-linux-<arch>.tar.gz` with a
SHA256 sum next to it. The CI workflow at
`.github/workflows/release-linux.yml` runs the same script on every
`v*` tag, attaches the tarball to the shared GitHub release, and
updates the Sparkle-shaped appcast at
`https://vu-releases.nowledge.co/appcast/<channel>-linux-x86_64.xml`
that the in-app notify-only updater polls.

## Useful Paths

- `crates/vu-app/src` — app shell and GPUI surfaces; the Cargo package and default binary are named `vu` (`cargo run -p vu`)
- `crates/vu-core/src` — shared app logic
- `docs/design` — design handoff set
- `docs/impl` — implementation notes
- `postmortem` — issue writeups and lessons learned
