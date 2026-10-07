# Muxer GUI

Muxer GUI is a standalone native Rust desktop app with the Muxer workspace
engine embedded. It shares Muxer's session files, so spaces and tabs are
available in both apps. Do not open the same session in both apps at once.

Start Muxer GUI from a project folder:

```sh
muxer-gui
```

It opens the local `default` session, or creates one in the current folder.
Session state is stored in the same local directory as Muxer. No `muxer`
binary, daemon, or local control server is needed. The GUI installer includes
the `solmu` runtime for Solmu spaces; start the Solmu backend before using
those spaces. Terminal spaces launch local shells and command-line clients.

The left sidebar shows spaces in both Terminal and Solmu views. When a tab has multiple panes,
they appear under **PANES** so you can switch between them; a lone pane needs
no extra sidebar entry. Tabs appear in a browser-style strip along the top of
the workspace. Select a tab to switch to it, use **+** to add one, or click its
**x** to close it. Split panes and send terminal input from the workspace. Click **+** beside
**Spaces** to choose a new **Terminal** or **Solmu** space. Right-click a space
in the sidebar and choose **Delete space** to remove it.
Solmu opens the shared native desktop interface, including its conversation
list and Profile, Audit, Tasks, and other pages. In Muxer GUI, use chat commands
such as `/plugins`, `/mcp`, `/skills`, `/compact`, `/status`, `/context`,
`/export`, `/copy`, `/rename`, and `/delete` instead of separate header buttons
and title controls. Choosing **Terminal** opens a
full interactive system shell
directly; it does not start the Solmu CLI. Click inside the terminal to type
and use shell shortcuts. Press **Ctrl+C** (or **Cmd+C** on macOS) to close its
terminal pane. You can run command-line clients such as Claude Code from that
shell.

Use **+** and enter a branch name. **Create worktree** creates a Git checkout
from the selected project's repository; **Open worktree** reopens that branch
as a separate Solmu space, focusing its existing space when it is already open.
See [Git worktrees](muxer-worktrees.md).

Muxer GUI owns the session lock while open. Close it before opening that
session in the terminal Muxer app; the session layout and metadata are saved
to the shared local state.

See [installation](releases.md#individual-components), [Muxer usage](muxer.md),
and [development](development.md).

![Muxer GUI](screenshots/muxer-gui.png)

![Creating a worktree from Muxer GUI](screenshots/muxer-gui-worktrees.png)
