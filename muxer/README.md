# Solmu muxer

A Rust TUI with project spaces and multiple Solmu tabs in each space.
Each tab holds a layout of real terminal panes. Each pane starts its own Solmu
conversation and keeps running while you switch tabs or spaces.

```sh
cargo build -p solmu-cli -p solmu-muxer
cargo run -p solmu-backend
# In another terminal:
cargo run -p solmu-muxer
```

Click **+ Space** to choose a project and **+ Tab** to add a session. Select
spaces in the sidebar and tabs along the top; **×** closes a tab. Each space
remembers its selected tab. Split panes right or down, zoom the focused pane,
and drag dividers to resize. Right-click a pane for its action menu.
Right-click spaces, tabs, or panes to rename them; names survive restarts.
**Find** searches names, paths, and statuses across the session. **Navigate**
lets you move without a prefix until Enter or Esc; **Help** lists shortcuts
and filters as you type. These controls also have **Ctrl+b g**, **m**, and **?**
shortcuts. Naming and search fields support Unicode cursor editing and paste.
**Settings** customizes shortcuts, themes, sidebar width, and working-directory
policies. Changes apply automatically, including edits to `config.toml`.
Read [Muxer settings](../docs/muxer-configuration.md) for examples.
Scripts can inspect sessions, create and arrange panes, send terminal input,
and read live screens through the [local automation commands](../docs/muxer-automation.md).
Event streams report live changes; waits observe pane states or matching output.
[Native prompt commands](../docs/muxer-agents.md) send messages without typing
into the active screen, queue tasks in order, and wait for their saved replies.
Stop cancels active and queued work while terminal and Profile drafts stay intact.
Press **Ctrl+b**, then **n** for a new tab, **Tab** to switch tabs, Up/Down to
switch spaces, **s** to split right, **-** to split down, or **q** to detach.
Use **Ctrl+b h/j/k/l** to focus panes, **H/J/K/L** to swap them, **z** to zoom,
**r** to resize with arrows, and **x** to close a pane. Closing a tab terminates
all its panes; saved conversations remain available.
Muxer runs a local background session: panes and replies keep running when you
detach or close the terminal. Run `muxer` again to reattach, or use
`muxer server stop` to stop the session. `muxer --session work` selects a named
session; `muxer session list` lists running sessions. Use `--foreground` for
a temporary session that ends when you quit.
After a server restart, Muxer restores spaces, tabs, and pane layouts, and
reopens existing Solmu conversations. Pending work stops when the server stops.
See the guide for saved state and recovery backups.
All Solmu conversation commands work inside panes, including `/stop` and `/exit`.

Run `muxer --help` or read the [muxer guide](../docs/muxer.md) for workspace
selection, configuration, shortcuts, and session lifetime.

![Solmu client screenshot](../docs/screenshots/muxer.png)

![Solmu prompt automation](../docs/screenshots/muxer-automation.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](../docs/tools.md) for usage and limits.
