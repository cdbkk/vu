# Terminal workflows

Vu organizes work as windows, tabs, panes, and terminal surfaces.

| Object | Purpose |
| --- | --- |
| Window | Independent workspace on a display |
| Tab | Group of related panes |
| Pane | Visible split region |
| Surface | Terminal session inside a pane |

Use tabs to separate projects or tasks. Use splits when two terminals must remain visible together. Use surfaces for multiple terminal sessions that can share one pane without making every split smaller.

## Input bar

The input bar runs commands in visible terminals. A single-pane tab targets its focused pane. In multi-pane tabs, the pane picker can target the focused pane, every pane, or a selected set.

Command history and local path completion appear inline. Tab or Right Arrow accepts a completion.

## Tabs and panes

Create tabs and splits from the File menu, Command Palette, or configured shortcuts. Drag tabs to reorder them. Drag a pane title into the tab strip to promote it to a tab.

Pane zoom temporarily lets the focused split fill the tab. Toggle it again to restore the split layout.

## Surfaces

A pane-local surface is a full terminal session. Use the surface rail to focus, rename, or close one. Creating a surface keeps the current pane geometry. Splitting a surface creates a new pane.

## Saved workspaces

Private sessions restore automatically. Project layout profiles in `.vu/workspace.toml` store tabs, panes, surfaces, split geometry, labels, and working directories without exporting scrollback or command history.

## Control plane

`vu-cli` can inspect tabs and panes, create or drive terminal surfaces, and use tmux targets through the running app's local control socket.
