# Redundant terminal and filesystem work

## What happened

The workspace scanned terminal surfaces every 8 ms while idle. Path completion read directories on the UI thread. Sidebar searches finished obsolete scans, and session saves captured screen text before the worker could combine requests.

## Root cause

The control bridge required polling, search cancellation only guarded result publication, and session coalescing happened after snapshot creation.

## Changes

macOS now wakes on Ghostty ticks and asynchronous control requests, with a 250 ms fallback for state without a notification. Each pump traverses surfaces once. Windows and Linux retain their 8 ms fallback.

Path completion runs after a 100 ms debounce on the background executor. It checks the current tab, surface, input, target and directory before publishing. Sidebar search waits 150 ms and checks cancellation while scanning. Session save requests share a 150 ms capture window; shutdown still cancels pending work and captures a fresh snapshot before flushing.

## Verification

`cargo check -p vu` passed. Tests passed in all three affected packages: 199 in `vu`, 84 in `vu-core`, and 11 in `vu-ghostty`. New regressions cover completion order and Unicode, cancelled searches, control-request delivery, and wake callback ordering.

Validation used `VU_GHOSTTY_SOURCE_DIR` with an existing build cache at the pinned upstream revision. The current build script detects a prebuilt library in the xcframework directory but its later lookup requires another output location. The build configuration was not changed.

Interactive latency, session restoration after restarting the app, and Windows/Linux execution remain unverified. The installed app was not replaced.

## Lesson

Cancel obsolete work before reading files, and combine save requests before capturing terminal state. Retain explicit wake sources and shutdown flushes when removing polling or delaying persistence.
