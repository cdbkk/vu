<p align="center">
  <img src="assets/Vu-macOS-Dark-256x256@2x.png" width="120" alt="vu app icon" />
</p>

<h1 align="center">vu</h1>

<p align="center"><strong>A better customizable terminal.</strong></p>

<p align="center">
  Native, GPU-accelerated, and built to feel like yours.
</p>

<p align="center">
  <a href="https://github.com/cdbkk/vu/actions/workflows/ci-portable.yml"><img alt="Build status" src="https://github.com/cdbkk/vu/actions/workflows/ci-portable.yml/badge.svg" /></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-FF9875" /></a>
  <img alt="macOS beta" src="https://img.shields.io/badge/macOS-beta-222222?logo=apple&logoColor=white" />
  <img alt="Windows beta" src="https://img.shields.io/badge/Windows-beta-0078D4?logo=windows&logoColor=white" />
  <img alt="Linux preview" src="https://img.shields.io/badge/Linux-preview-FCC624?logo=linux&logoColor=111111" />
</p>

<p align="center">
  <a href="docs/media/vu-demo.mp4">
    <img src="docs/media/vu-demo.gif" width="1080" alt="vu building a live four-pane workspace with lazygit, yazi, repository activity, and a real 89-test Rust suite" />
  </a>
</p>

<p align="center"><sub>Click the demo for the full-quality video.</sub></p>

## A better customizable terminal

`vu` is for people who want a real terminal that looks and works like theirs.
It keeps the speed and behavior of a native terminal, then makes the parts you
touch every day easy to tune live — no dotfile archaeology required.

### Make it yours

- 15 built-in themes with live previews
- a full ANSI palette editor with direct color picking
- Ghostty theme import and export
- separate terminal and interface fonts
- opacity, blur, background images, tab position, pane chrome, and icon scale
- the terminal options power users expect, exposed in one place

### Stay terminal-first

- native GPU rendering: Metal on macOS, D3D11/DirectWrite on Windows
- tabs, split panes, pane zoom, drag-to-rearrange, and command broadcasting
- first-class `ssh`, `tmux`, shells, editors, and TUIs
- configurable shortcuts and a command palette
- a local `vu-cli` control plane for scripts, tests, and external tools

## Status

`vu` is in active beta development.

| Platform | Status | Backend |
| --- | --- | --- |
| macOS | Beta, primary target | libghostty + Metal |
| Windows | Beta | ConPTY + libghostty-vt + D3D11/DirectWrite |
| Linux | Preview | Unix PTY + libghostty-vt + GPUI |

## Install

Download the unsigned macOS beta for Apple silicon (M1 or newer):

- [Download the DMG](https://github.com/cdbkk/vu/releases/download/v0.4.0-beta.1/vu-Beta-0.4.0-beta.1-macos-arm64.dmg)

> **Unsigned beta:** Apple has not notarized this build. After trying to open
> it, go to **System Settings → Privacy & Security** and click **Open Anyway**.

Open the DMG, then drag **vu Beta** to Applications. The checksum and ZIP build
are available on the [release page](https://github.com/cdbkk/vu/releases/tag/v0.4.0-beta.1).

## Build from source

The repository pins its toolchain with [mise](https://mise.jdx.dev/):

```sh
git clone https://github.com/cdbkk/vu.git
cd vu
mise install
mise exec -- cargo build --release -p vu
```

On macOS 26, build the app bundle through the included Zig SDK shim:

```sh
VU_ZIG_BIN="$PWD/scripts/zig-macos26-shim.sh" \
VU_ZIG_REAL="$(mise where zig)/bin/zig" \
mise exec -- just channel=dev macos-bundle

open "dist/macos/dev/arm64/vu Dev.app"
```

See the [install guide](docs/install.md) for platform prerequisites and
[HACKING.md](HACKING.md) for contributor builds, tests, and release tooling.

## Start here

- [Quick controls](docs/quick-controls.md)
- [Appearance and settings](docs/settings.md)
- [Terminal workflows](docs/terminal-workflows.md)
- [CLI and control plane](docs/cli.md)
- [Architecture](DESIGN.md)
- [Release notes](CHANGELOG.md)

## Built on excellent work

`vu` builds on [Ghostty](https://github.com/ghostty-org/ghostty),
[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui),
[gpui-component](https://github.com/longbridge/gpui-component),
[Phosphor Icons](https://phosphoricons.com/), and
[Flexoki](https://stephango.com/flexoki).

## License

[MIT](LICENSE)
