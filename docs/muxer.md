# Solmu muxer

Muxer groups **real Solmu CLI terminals** into spaces on Linux, macOS, and
Windows. Each space is a working directory and can hold up to eight tabs;
up to eight spaces can be open. Every tab creates a new conversation and keeps
running while you switch tabs or spaces. The sidebar shows spaces and their
tab counts; tabs show activity, and the focused terminal shows its status.

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
tab. Click a tab to focus it, or its **×** icon to close and terminate that CLI.
Closing a background tab preserves the current tab. Closing a space's last
tab removes the space; closing the final tab quits Muxer. Saved conversations
remain in the backend. Toolbar buttons also create spaces/tabs, toggle split
view, restart an exited CLI, or quit.

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
| `s` | Toggle between focused view and two tabs side by side |
| `x` | Close and terminate the selected CLI |
| `r` | Restart an exited CLI; creates a new conversation |
| `q` | Quit muxer and terminate its CLIs |
| `b` | Send a literal Ctrl+b to the selected CLI |

Only the focused tab receives typing. Split view shows the focused tab and
the next tab within the same space. Click a visible terminal to focus it.
Terminal dimensions follow resizing and view changes; tab strips keep the
selected tab visible in narrow windows.

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

This first implementation supports **Solmu only** and foreground sessions.
There is no background server or detach/reattach yet. Closing muxer terminates
its launched CLIs; conversations remain saved in the backend. `/exit` leaves
an exited pane visible so its final terminal contents can be read or restarted.

![Solmu client screenshot](screenshots/muxer.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](tools.md) for usage and limits.
