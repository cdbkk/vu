# Settings

Open Settings with Cmd-Comma on macOS or Ctrl-Comma on Windows and Linux.

## General

General contains app-level behavior, proxy settings, and update information.

## Appearance

Appearance controls terminal and interface fonts, font sizes, cursor style, opacity, blur, background images, tab placement, pane chrome, and icon size. Theme changes preview immediately. Save to keep them.

The terminal font must be a concrete installed family. GPUI virtual families such as `.SystemUIFont` apply only to interface text.

## Terminal colors

Choose a built-in theme, import a Ghostty theme, or edit the ANSI palette. The palette editor changes the active preview without rewriting the source theme until you save.

## Shortcuts

Keys lists editable application, tab, pane, surface, sidebar, and input-bar shortcuts. Shortcut recording rejects conflicts with active application bindings.

The optional global Summon / Hide Vu shortcut is disabled by default because it may conflict with launchers or window managers.

## Network

Proxy fields set `HTTP_PROXY` and `HTTPS_PROXY` for Vu's network requests. Empty fields leave the inherited environment unchanged.
