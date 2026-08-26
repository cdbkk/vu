# `vu-cli` E2E Guide

This is the reference workflow for validating Vu's local control plane against a real running app session.

Use it for:

- smoke tests after control-plane changes
- automated client evaluation runs
- reproducible bug reports for `vu-cli` and the Unix socket bridge

## Preconditions

- Buildable workspace
- A macOS session that can launch the Vu window

## 1. Launch the app

```bash
RUST_LOG=vu=debug cargo run -p vu
```

Wait for the socket:

```bash
test -S /tmp/vu-debug.sock
```

Debug builds use `/tmp/vu-debug.sock`; release builds use `/tmp/vu.sock`.
`vu-cli` uses the same build-aware default when built from the workspace.

Override the socket only when needed:

```bash
VU_SOCKET_PATH=/tmp/vu-alt.sock cargo run -p vu
vu-cli --socket /tmp/vu-alt.sock identify
```

## 2. Baseline discovery

Prefer `--json` for automation:

```bash
vu-cli --json identify
vu-cli --json tabs list
vu-cli --json panes list
vu-cli --json tree
```

Before driving a pane, confirm it actually exposes the capability you need.

For visible shell execution, the target pane should expose:

- `exec_visible_shell`
- usually `probe_shell_context` too

If those are missing, do not send shell commands through `panes exec`.

## 3. Visible shell smoke test

Example:

```bash
vu-cli --json panes exec --tab 2 --pane-id 0 -- /bin/echo READY_OK
vu-cli --json panes wait --tab 2 --pane-id 0 --pattern READY_OK --timeout 10
vu-cli --json panes list --tab 2
```

Expected shape:

- `panes exec` returns visible output from the real pane
- `panes wait` returns `"status":"matched"`
- the pane remains in `shell_prompt` mode with `exec_visible_shell`

## 4. Pane creation

Current command:

```bash
vu-cli --json panes create --tab 2 --location right
```

Current status as of April 10, 2026:

- the request now returns promptly
- the returned pane identity is stable
- the response now reports `surface_ready`, `is_alive`, and `has_shell_integration`

Treat startup-command panes as provisional until their foreground state settles, but a clean pane creation should now come back with a live initialized surface:

```bash
vu-cli --json panes list --tab 2
vu-cli --json panes read --tab 2 --pane-id <new_id> --lines 40
```

For follow-up shell control, wait until the pane reports:

- `surface_ready: true`
- `is_alive: true`
- the expected shell control capabilities if you need visible shell execution

## 5. Automation guidance

- Use `pane_id` for follow-up targeting, not `pane_index`
- Use `surfaces.*` only when the test is explicitly about pane-local terminal
  sessions.
- Use `surface_id` for surface follow-up targeting.
- After `surfaces create` or `surfaces split`, call
  `surfaces wait-ready --surface-id <id> --timeout 10` before sending input that
  assumes an initialized shell.
- Keep assertions concrete and machine-checkable
- Record the exact tab index you used
- Capture both the command result and a follow-up `panes list` snapshot when reporting regressions

## Suggested eval sequence

```bash
vu-cli --json identify
vu-cli --json tabs list
vu-cli --json panes list --tab 2
vu-cli --json panes exec --tab 2 --pane-id 0 -- /bin/echo READY_OK
vu-cli --json panes wait --tab 2 --pane-id 0 --pattern READY_OK --timeout 10
vu-cli --json panes list --tab 2
```
