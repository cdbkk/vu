# Implementation: Socket API and `vu-cli`

## Overview

vu now ships a real local control plane:

- the app listens on a Unix domain socket
- requests use newline-delimited JSON-RPC 2.0
- `vu-cli` is the first client on top of that socket

This is the first automation slice, not the final surface. The important architectural move is that the CLI does not invent new terminal semantics. It routes into the same pane runtime, tmux adapter, visible-shell execution guard, and per-tab agent session that the built-in UI already uses.

That keeps CLI automation honest: if a pane is not a proven shell in the app, `vu-cli panes exec` is refused for the same reason.

## Local references that shaped the design

The CLI shape is intentionally informed by the local 3pp references in this repo:

- `3pp/agent-browser` — flat agent-friendly command ergonomics, strong `--json` output, and a CLI that is useful both for humans and other agents
- `3pp/waveterm/cmd/wsh` — app control over a local socket with a small RPC boundary instead of bolting automation directly onto UI code
- `3pp/cmux/CLI/cmux.swift` — explicit socket-path discovery and the idea that the socket client should stay thin while the app remains the authority

The result in vu is a hybrid:

- user-facing CLI commands are grouped by domain: `tabs`, `panes`, `tmux`, `agent`
- the transport methods are namespaced JSON-RPC methods such as `panes.list` and `agent.ask`

## Socket path

- Release default: `/tmp/vu.sock`
- Debug default: `/tmp/vu-debug.sock`
- Override: `VU_SOCKET_PATH`
- CLI override: `vu-cli --socket /custom/path ...`

The app removes stale socket files on startup and creates the live socket with
user-only permissions. Debug builds intentionally use a separate default
endpoint so `cargo run -p vu` can run beside an installed Vu without making
startup treat the dev process as another window for the production app.

## Protocol

JSON-RPC 2.0 over a Unix domain socket, one JSON request per line and one JSON response per line.

### Request

```json
{"jsonrpc":"2.0","id":"req-1","method":"panes.read","params":{"tab_index":1,"pane_id":3,"lines":80}}
```

### Response

```json
{"jsonrpc":"2.0","id":"req-1","result":{"content":"..."}}
```

### Error

```json
{"jsonrpc":"2.0","id":"req-1","error":{"code":-32602,"message":"Pane index 9 is out of range. Valid panes are ..."}}
```

## Current method surface

### System

- `system.identify`
- `system.capabilities`

### Tabs

- `tabs.list`

### Panes

- `panes.list`
- `panes.read`
- `panes.exec`
- `panes.send_keys`
- `panes.create`
- `panes.wait`
- `panes.probe_shell`

### Tree / Surfaces

- `tree.get`
- `surfaces.list`
- `surfaces.create`
- `surfaces.split`
- `surfaces.focus`
- `surfaces.rename`
- `surfaces.close`
- `surfaces.read`
- `surfaces.send_text`
- `surfaces.send_key`
- `surfaces.wait_ready`

### tmux

- `tmux.inspect`
- `tmux.list`
- `tmux.capture`
- `tmux.send_keys`
- `tmux.run`

### Agent

- `agent.ask`
- `agent.new_conversation`

## CLI surface

The first shipped client is `vu-cli`.

Examples:

```bash
vu-cli identify
vu-cli capabilities

vu-cli tabs list
vu-cli panes list --tab 1
vu-cli panes read --tab 1 --pane-id 3 --lines 120
vu-cli panes exec --tab 1 --pane-id 3 cargo test -q
vu-cli panes send-keys --tab 1 --pane-id 3 $'\u0003'
vu-cli panes create --tab 1 --location right --command "htop"
vu-cli panes wait --tab 1 --pane-id 3 --timeout 20

vu-cli tree --tab 1
vu-cli surfaces list --tab 1
vu-cli surfaces split --tab 1 --pane-id 3 --location right --title worker-1 --owner subagent --command "codex"
vu-cli surfaces create --tab 1 --pane-id 4 --title worker-2 --owner subagent --command "codex"
vu-cli surfaces wait-ready --tab 1 --surface-id 7 --timeout 10
vu-cli surfaces focus --tab 1 --surface-id 7
vu-cli surfaces send-text --surface-id 7 "explain this repo"
vu-cli surfaces send-key --surface-id 7 enter
vu-cli surfaces read --surface-id 7 --lines 120
vu-cli surfaces close --surface-id 7 --close-empty-owned-pane

vu-cli tmux list --tab 1 --pane-id 3
vu-cli tmux capture --tab 1 --pane-id 3 --target %17 --lines 80
vu-cli tmux run --tab 1 --pane-id 3 --location new-window -- cargo test

vu-cli agent ask --tab 1 "Summarize what is happening in this tab"
vu-cli agent ask --tab 1 --auto-approve-tools "Run the test suite and explain failures"
vu-cli agent new-conversation --tab 1
```

For automation, every command also supports `--json`.

## Request routing rules

### Tabs are explicit

- `tab_index` is 1-based in the control API and CLI
- omitting `tab_index` means "use the active tab"

### Pane targets reuse vu's stable pane model

- `pane_index` is the current visible position in the split layout
- `pane_id` is the stable identity for the life of that pane inside the tab
- follow-up automation should prefer `pane_id`

### Surfaces are opt-in pane-local sessions

- a pane is a visible split rectangle
- a surface is a live terminal session inside a pane
- every pane has one active surface
- `panes.*` continues to operate only on the active surface
- `surfaces.*` exposes additional pane-local tabs without changing pane
  semantics for the built-in agent or existing benchmarks
- follow-up surface automation should prefer `surface_id`

See `docs/impl/pane-surfaces.md` for the full compatibility contract.

### Pane actions reuse existing guards

- `panes.exec` goes through the same visible-shell execution path as the built-in agent
- `tmux.*` methods only work when the pane exposes native tmux capability
- `panes.probe_shell` only works when the pane exposes `probe_shell_context`

That is deliberate. The CLI is not allowed to bypass the control-plane truths that the UI and agent already depend on.

## Agent session behavior

`agent.ask` targets a tab's existing built-in agent session.

That means:

- it reuses the tab's conversation history
- it reuses the tab's focused-pane context and peer-pane summary
- the response also appears in vu's own agent UI for that tab

This is what makes CLI-driven end-to-end evaluation possible. A coding agent outside the app can drive panes, inspect tmux state, call the built-in agent in a real tab session, and verify the visible result without a human acting as the bridge.

## Threading model

- socket accept/read/write runs on the harness Tokio runtime
- JSON-RPC parsing happens off the GPUI render path
- requests are bridged onto the workspace over a channel
- pane/tmux operations still execute through the workspace, because the workspace owns live panes
- long-running results are delivered back asynchronously over one-shot replies

The render loop stays authoritative, but the socket never blocks on it directly.

## Known limits of the first slice

- no subscription or streaming RPC yet; requests are request/response only
- no window-focus or pane-focus RPC yet
- no auth modes beyond local socket file permissions yet
- `agent.ask` assumes one automation request at a time per tab
- the socket surface only exposes capabilities that already exist in vu's runtime and agent layers; it does not yet add a new app-native Codex/OpenCode attachment
- pane-local surfaces are live-session control primitives; session restore
  persists the active pane layout as before, not every hidden live surface

Those are acceptable limits for phase one. The important part is that the control transport now exists and is wired to real product semantics instead of placeholder CLI text.

## Live E2E status

Live app-backed smoke tests were rerun on April 10, 2026 against a real `cargo run -p vu` session.

Verified end to end:

- `vu-cli identify`
- `vu-cli tabs list`
- `vu-cli panes list`
- `vu-cli panes read`
- `vu-cli panes exec`
- `vu-cli panes wait`
- `vu-cli agent ask`

Verified behavior notes:

- `panes exec` now recovers cleanly when the visible prompt is back but Ghostty shell integration does not emit a matching command-finished signal fast enough for automation. That keeps the pane usable for follow-up `panes exec` calls in the same session.
- `agent ask` was verified against a real configured model session, not just the transport boundary.

Current known limitation from the same live run:

- `panes.create` returns promptly with `tab_index`, `pane_index`, `pane_id`, `surface_ready`, `is_alive`, and `has_shell_integration`. Use `pane_id` for follow-up targeting, then confirm the created pane exposes the exact control capabilities you need before driving it further.
