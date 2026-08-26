# Socket API

Vu serves JSON-RPC 2.0 over a local platform transport:

- Unix domain socket on macOS and Linux
- named pipe on Windows

Each request and response is one JSON object. `vu-cli` is a thin client over the same protocol.

## Method groups

- `system.identify`, `system.capabilities`
- `tabs.list`, `tabs.new`, `tabs.close`
- `panes.list`, `panes.read`, `panes.exec`, `panes.send_keys`, `panes.create`, `panes.wait`, `panes.probe_shell`
- `tree.get`
- `surfaces.list`, `surfaces.create`, `surfaces.split`, `surfaces.focus`, `surfaces.rename`, `surfaces.close`, `surfaces.read`, `surfaces.send_text`, `surfaces.send_key`, `surfaces.wait_ready`
- `tmux.inspect`, `tmux.list`, `tmux.capture`, `tmux.send_keys`, `tmux.run`

## Targeting

Tabs use one-based indexes. Pane requests accept either a one-based pane index or a stable pane id. Surface requests accept a surface id and optional pane target.

A pane index always addresses the outer Vu pane. tmux methods use tmux target identifiers for windows and panes inside that terminal.

## Execution

Pane and surface commands write through the visible PTY. They do not start hidden shell subprocesses. Readiness and wait operations observe the same terminal state shown in the app.

## Concurrency

The app owns terminal mutation. Socket tasks decode requests, send them to the workspace, and await a one-shot response. Window-aware mutations return to the GPUI context before changing tabs, panes, or surfaces.
