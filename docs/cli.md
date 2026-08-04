# vu-cli and surfaces

`vu-cli` is the command-line handle for a running vu app. It is installed with
vu on macOS, Windows, and Linux.

Most users do not need it. Use it when another program, script, test runner, or
agent needs to inspect the visible terminal, create panes, drive pane-local
surfaces, or ask vu's built-in agent.

External agents should get a real terminal to work with, not a headless shell
that behaves differently from what the user sees.

## Check that vu is reachable

Start vu first, then run:

```sh
vu-cli identify
vu-cli tabs list
vu-cli panes list
```

For scripts and agent adapters, prefer JSON:

```sh
vu-cli --json identify
vu-cli --json tree
```

`vu-cli` asks the running app for state instead of guessing from outside the
terminal.

## Pane commands

Read terminal content:

```sh
vu-cli --json panes read --tab 1 --pane-id 0 --lines 120
```

Create a visible split:

```sh
vu-cli --json panes create --tab 1 --location right --command "htop"
```

Run a command visibly in a shell pane:

```sh
vu-cli --json panes exec --tab 1 --pane-id 0 -- cargo test -q
```

Use `pane_id` for follow-up automation. Pane indexes can change when a user
splits, closes, or rearranges panes.

## Surface commands

A pane is a visible split region. A surface is a terminal session inside one
pane. Every pane has one active surface, and a pane can host more surfaces when
you want pane-local tabs.

This is the pattern most subagent tools want:

1. Create the first worker as a visible split.
2. Add later workers as surfaces inside that worker pane.
3. Drive each worker by `surface_id`.
4. Close each worker surface when it finishes.

Create the first worker pane:

```sh
vu-cli --json surfaces split \
  --tab 1 \
  --pane-id 0 \
  --location right \
  --title worker-1 \
  --owner pi-interactive-subagents \
  --command "codex"
```

Create another worker inside the same pane:

```sh
vu-cli --json surfaces create \
  --tab 1 \
  --pane-id <worker_pane_id> \
  --title worker-2 \
  --owner pi-interactive-subagents \
  --command "codex"
```

Wait before sending input:

```sh
vu-cli --json surfaces wait-ready --surface-id <surface_id> --timeout 10
```

Drive the worker:

```sh
vu-cli --json surfaces send-text --surface-id <surface_id> "explain this repo"
vu-cli --json surfaces send-key --surface-id <surface_id> enter
vu-cli --json surfaces read --surface-id <surface_id> --lines 120
```

Close it:

```sh
vu-cli --json surfaces close --surface-id <surface_id>
```

If a worker pane was created only for owned surfaces, an orchestrator can ask vu
to close that pane when its last surface closes:

```sh
vu-cli --json surfaces close \
  --surface-id <surface_id> \
  --close-empty-owned-pane
```

## Ask the built-in agent from a script

`agent ask` uses the same in-app agent session you see in the side panel:

```sh
vu-cli --json agent ask --tab 1 "Summarize what is happening in this tab"
```

That means the response appears in vu, uses the tab's current context, and
follows the same provider and approval settings as the UI.

## Automation rules

- Use `--json` for anything a program will parse.
- Use `pane_id` and `surface_id` after discovery.
- Call `surfaces wait-ready` before driving a new surface.
- Treat `panes exec` as visible shell control, not a hidden subprocess runner.
- Give orchestrator-created surfaces an `--owner` so cleanup can be scoped.
- Re-read `tree` after user-visible layout changes.

The socket is local to the user session. It is not a remote API, and it does not
change shell permissions. If a script can drive `vu-cli`, it can drive the
visible terminal you already opened.

## Socket path

Release builds use the default socket for the installed channel. Debug builds
use a separate debug socket so a local development build can run beside the
installed app.

Set `VU_SOCKET_PATH` only when you intentionally run vu on a custom endpoint:

```sh
VU_SOCKET_PATH=/tmp/vu-alt.sock vu
vu-cli --socket /tmp/vu-alt.sock identify
```
