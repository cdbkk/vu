# Tab shortcuts and command palette labels

## What happened

The bracket keys appeared to do nothing in a window with two main tabs and one
split pane. After assigning them to tab navigation, the command palette still
showed a fixed shortcut alias instead of the configured brackets.

## Root cause

The saved brackets were assigned to split-pane focus, which has no visible
effect with one pane. Tab navigation uses separate actions. The palette also
used hardcoded shortcut labels. Reading the registered bindings exposed a
second issue: fixed aliases registered after configurable bindings took display
precedence.

## Fix applied

The user's saved bindings now assign `[` to the previous main tab and `]` to the
next main tab. These are personal settings, not new application defaults.
The palette reads active action bindings, and configurable bindings register
after fixed aliases so their labels take precedence. Unassigned shortcuts in
settings display "Not set" and remain clickable.

`VU_CONFIG_PATH` allows a demo or test process to use its own preferences file.
Without the override, configuration uses the existing platform path.

## Verification

The display-precedence regression failed before the registration-order fix and
passed afterward. The app, core, and paths test suites passed 310 tests, with one
benchmark ignored. In a headless macOS VM, native bracket keypresses switched
between two main tabs and the palette displayed the configured brackets.

The 42-second demo records the built app inside that VM using sample files and
isolated settings. Its MP4 and GIF decode successfully. The VM is stopped after
capture.

## What we learned

Verify tab navigation with multiple main tabs and one split pane. Check both
the action and the shortcut shown to the user. Desktop recording and keyboard
automation belong in an isolated guest session so they cannot take control of
the user's working window.
