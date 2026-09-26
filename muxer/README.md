# Solmu muxer

A Rust TUI with project spaces and multiple Solmu tabs in each space.
Each tab owns a real terminal and creates its own conversation on startup.

```sh
cargo build -p solmu-cli -p solmu-muxer
cargo run -p solmu-backend
# In another terminal:
cargo run -p solmu-muxer
```

Click **+ Space** to choose a project and **+ Tab** to add a session. Select
spaces in the sidebar and tabs along the top; **×** closes a tab. Each space
remembers its selected tab. The toolbar offers split view, restart, and quit.
Press **Ctrl+b**, then **n** for a new tab, **Tab** to switch tabs, Up/Down to
switch spaces, **s** for split view, or **q** to quit.
All Solmu conversation commands work inside panes, including `/stop` and `/exit`.

Run `muxer --help` or read the [muxer guide](../docs/muxer.md) for workspace
selection, configuration, shortcuts, and session lifetime.

![Solmu client screenshot](../docs/screenshots/muxer.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](../docs/tools.md) for usage and limits.
