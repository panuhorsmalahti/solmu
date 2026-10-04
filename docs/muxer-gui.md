# Muxer GUI

Muxer GUI is a native Rust desktop companion for [Muxer](muxer.md). It uses
Muxer's local control interface, so the same session can be opened in the GUI
and the terminal client.

With the backend running, start Muxer GUI from a project folder:

```sh
muxer-gui
```

Start the Muxer session from the window if one is not running. The window
refreshes session state automatically. Select a space, tab, or pane; create
spaces and tabs; split a pane; view its terminal screen; send a line of input;
or close a pane. Set `SOLMU_MUXER_PATH` if the `muxer` executable is not beside
`muxer-gui` or on `PATH`.

The sidebar shows the active session, its spaces, tabs, and panes. The main
panel keeps the selected terminal output and command field together; the toolbar
lets you add a tab, split the active pane, or close it. Session and workspace
settings stay in the sidebar, and the connected state updates automatically.

The terminal Muxer remains available as `muxer` and includes pane resizing,
shell and command panes, settings, automation, and direct terminal attachment.
See [Muxer usage](muxer.md), [installation](releases.md#individual-components),
and [development](development.md).

![Muxer GUI](screenshots/muxer-gui.png)
