# Settings

Open Settings with Cmd-Comma on macOS or Ctrl-Comma on Windows and Linux.
Three tabs. Changes preview on the workspace behind the window, and **Save**
writes them to `config.toml`.

## General

Where new tabs start, whether terminal text comes back after a restart, the
update checker, and HTTP proxy fields. The **Open config.toml** button opens
the file itself.

## Appearance

The theme picker sits at the top, with **Import Theme** for pasting a Ghostty
theme and **Customize** for the palette editor. Below it are the terminal and
interface fonts, the terminal tweaks, and four cards. Transparency covers
opacity and blur. Tabs & Top Bar covers tab placement, accents, and strip
colors. Background Image is a per terminal image on macOS. Panes covers title
bars, icon scale, and chrome strength.

The terminal font must be an installed family. Virtual names such as
`.SystemUIFont` apply only to interface text.

## Keys

Every rebindable shortcut, the fixed ones vu reserves, and the optional global
Summon / Hide vu shortcut. Click a shortcut and press a new combination to
record it. Save rejects a combination another action already uses.

The full list of keys, ranges, theme file format, and a complete annotated
`config.toml` are in [Customization](customization.md).
