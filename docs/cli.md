# vu-cli

`vu-cli` controls a running Vu process through its local JSON-RPC socket.

Use `--json` for scripts:

```sh
vu-cli --json identify
vu-cli --json tabs list
vu-cli --json panes list --tab 1
```

## Panes

```sh
vu-cli panes read --tab 1 --pane-index 1 --lines 80
vu-cli panes send-keys --tab 1 --pane-index 1 --keys "git status\n"
vu-cli panes create --tab 1 --location right
vu-cli panes wait --tab 1 --pane-index 2 --timeout 10
```

Pane indexes address Vu's visible panes. They do not address nested tmux panes.

## Surfaces

```sh
vu-cli --json surfaces list --tab 1
vu-cli --json surfaces create --tab 1 --pane-index 1 --title logs
vu-cli --json surfaces wait-ready --surface-id 2 --timeout 10
vu-cli surfaces send-text --surface-id 2 "tail -f app.log\n"
vu-cli surfaces read --surface-id 2 --lines 80
```

Wait for a new surface before sending input that assumes its shell has initialized.

## tmux

```sh
vu-cli --json tmux inspect --pane-index 1
vu-cli --json tmux list --pane-index 1
vu-cli tmux capture --pane-index 1 --target %3 --lines 80
vu-cli tmux send-keys --pane-index 1 --target %3 --literal-text "git status" --enter
```

## Socket path

Release builds use `/tmp/vu.sock` on Unix and `\\.\pipe\vu` on Windows. Debug builds use a separate endpoint. Set `VU_SOCKET_PATH` or pass `--socket` to override it.
