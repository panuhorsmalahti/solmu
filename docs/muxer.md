# Solmu muxer

Muxer groups **real Solmu CLI terminals** into spaces on Linux, macOS, and
Windows. Each space is a working directory and can hold up to eight tabs;
up to eight spaces can be open. Each tab holds up to eight real terminal panes
in a nested layout. Every pane starts its own conversation and keeps running
while you switch tabs or spaces. The sidebar shows spaces and their tab counts;
tabs show activity, and each terminal shows its status.

Start the backend, then build and launch from the repository root:

```sh
cargo run -p solmu-backend
# In another terminal:
cargo build -p solmu-cli -p solmu-muxer
cargo run -p solmu-muxer
```

Installed releases provide the `muxer` command. Select initial workspaces with
one or more `--cwd PATH` options; paths containing spaces must be quoted:

```sh
muxer --cwd /path/to/project --cwd "/path/to/another project"
```

The default workspace is your working directory. **Ctrl+b w** opens a path
prompt for a new space; Enter or **Create** starts its first tab and Esc or
**Cancel** dismisses it. Directories must already exist. **Ctrl+b n** or **+ Tab**
starts another tab in the current space. New tabs are selected automatically.

Click a sidebar space to switch projects. Each space remembers its selected
tab and each tab remembers its focused pane. Click a tab to select its layout,
or its **×** icon to close and terminate all its CLIs. Closing a background tab
preserves the current tab. Closing a space's last tab removes the space;
closing the final tab ends that session. Saved conversations remain in the backend.
Toolbar buttons also create spaces/tabs, open split actions, zoom the focused
pane, restart an exited CLI, or detach.

Split a pane **right** or **down** to start another Solmu conversation in the
same workspace. Splits can be nested. Click any visible terminal to focus it;
only the focused pane receives typing. Right-click a pane for split, zoom,
resize, close, and restart actions. Its header's **×** closes just that pane;
the remaining layout expands to fill the gap.

Drag a divider to resize its adjacent panes. **Ctrl+b r** enters keyboard
resizing for a running pane: arrows or `h/j/k/l` adjust its nearest divider
by five percent; Enter or Esc finishes. **Ctrl+b z** zooms a pane to fill the
tab, and repeats to restore its layout. Terminal dimensions follow resizing
and view changes; tab strips keep the selected tab visible in narrow windows.

## Controls

Press **Ctrl+b**, release it, then press the second key:

| Second key | Action |
| --- | --- |
| `n` | New Solmu tab in the current space |
| `w` | New space path prompt |
| Up / Down | Previous / next space |
| Tab or `]` | Next tab in the space |
| Shift+Tab or `[` | Previous tab in the space |
| `1`–`8` | Select a tab by its position within the space |
| `s` or `v` | Split the focused pane to the right |
| `-` | Split the focused pane down |
| `h/j/k/l` | Focus the pane to the left/down/up/right |
| `H/J/K/L` | Swap the focused pane with its neighbor |
| Left / Right | Focus a horizontal neighbor; Shift swaps it |
| `z` | Zoom the focused pane or restore the layout |
| `x` | Close and terminate the focused pane |
| `X` | Close the selected tab and all its panes |
| `r` | Resize a running pane; restart an exited CLI with a new conversation |
| `q` | Detach from the session; its CLIs keep running |
| `b` | Send a literal Ctrl+b to the selected CLI |

Use all [CLI conversation commands](cli.md) inside a pane: send messages,
receive streaming replies, create/open/rename/delete threads, complete commands
with Tab, stop with Esc or `/stop`, and quit that CLI with `/exit`. Enter pressed
during a finishing conversation operation or live refresh is preserved until
the CLI is ready. Saved conversations and WebSocket updates are shared with
desktop and web too. `/profile` edits the shared system prompt and optional
default model; `/model` selects a model for the current thread. Tabs use their
space's directory as the thread workspace.

## Configuration and lifetime

`SOLMU_BACKEND_URL` defaults to `http://127.0.0.1:3000`. Muxer loads `.env`
from its working directory; existing environment variables take precedence.
Provider keys stay on the backend. Muxer finds `solmu-cli` beside its own
executable, then on PATH. Set `SOLMU_CLI_PATH` to an absolute Solmu CLI path
if installed elsewhere.

Muxer supports **Solmu only**. Running `muxer` starts or attaches to your
**default local background session**. **Detach** or **Ctrl+b q** closes only
the attached UI: its panes keep running, including streaming replies. Closing
the terminal also leaves the session running. Run `muxer` again to reattach
to the same live terminals and conversations.

Use named sessions for separate groups of projects:

```sh
muxer session attach work
muxer --session side-project --cwd /path/to/project
muxer session list
muxer server status --session work
muxer server stop --session work
muxer pane read 1 --session work
```

`muxer server start --session work` starts a session without opening a UI.
`--cwd` options set initial spaces for a new session; existing or restored
sessions use their saved spaces. Add more projects with **+ Space** after attaching.
Session names accept letters, digits, underscores, and hyphens (up to 64 characters).
`muxer pane read ID` prints the live pane's visible terminal text, including
when no UI is attached. Pane IDs appear in terminal headers. This command reads
the current screen; it does not create a conversation or send input.

Several terminals can attach to one session. Each remembers its own selected
space, tab, pane, and zoom. Shared layout changes appear automatically in all
clients. When clients view different tabs, each tab follows its viewer's size.
When they share a tab, the last client to interact controls its pane dimensions;
other clients display as much as fits in their terminal.

Stop the default session and its CLIs with `muxer server stop`. `/exit` leaves
an exited pane visible so its final contents can be read or restarted. Saved
conversations remain available after stopping. For a temporary session whose
panes terminate when you quit, use `muxer --foreground`.

Session connection state and error logs live under `~/.solmu/muxer`
(`%USERPROFILE%\.solmu\muxer` on Windows); `SOLMU_MUXER_DIR` overrides that
directory. Keep it private to your account. The server listens only on localhost
and authenticates clients using a per-session token.

## Restart recovery

Muxer saves each session's spaces, working directories, tabs, nested splits,
divider sizes, zoom, and last active pane in `<session>.json` in its state
directory. Closing a tab or pane updates the saved layout. Closing the final
tab ends the session; its next launch starts a new layout.

After a server stop or restart, running Solmu panes reopen their saved
conversations, including message history, tool results, and model selections.
New processes are started: pending replies and commands do not continue across
a server restart. Detaching keeps those original processes alive.
Panes previously closed with `/exit` remain stopped; use **Restart** to start a
new conversation. A missing workspace is shown as unavailable instead of
silently substituting a different working directory.

Only layout and conversation references are written to Muxer's snapshot;
terminal contents and message text are not copied into it. Conversation history
stays in the backend database.

If a snapshot is corrupt or belongs to an unsupported format, Muxer preserves
its original bytes in `backups/` before replacing it with a fresh session.
The latest three recovery copies per session are retained. If preservation
fails, the original snapshot remains untouched; running panes still work.
Recovery details are written to `<session>.log`. To recover a copy, stop that
session, copy the chosen backup over `<session>.json`, and start it again.

![Solmu client screenshot](screenshots/muxer.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](tools.md) for usage and limits.
