# Solmu muxer

Muxer runs up to eight **real Solmu CLI terminals** in one Rust TUI, on Linux,
macOS, and Windows. Every pane creates a new conversation and keeps running
while you focus another pane. The sidebar shows each workspace and whether
Solmu is starting, working, idle, showing an error, or exited.

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
prompt for a new workspace; Enter starts a Solmu pane there and Esc cancels.
Directories must already exist. **Ctrl+b n** starts another pane in the current
workspace. New panes are selected automatically.

## Controls

Press **Ctrl+b**, release it, then press the second key:

| Second key | Action |
| --- | --- |
| `n` | New Solmu pane in the current workspace |
| `w` | New workspace path prompt |
| Tab or `]` | Next pane |
| Shift+Tab or `[` | Previous pane |
| `1`–`8` | Select a pane by its sidebar position |
| `s` | Toggle between focused view and two panes side by side |
| `x` | Close and terminate the selected CLI |
| `r` | Restart an exited CLI; creates a new conversation |
| `q` | Quit muxer and terminate its CLIs |
| `b` | Send a literal Ctrl+b to the selected CLI |

Click a sidebar entry or visible pane to focus it. Only the focused pane
receives typing. Split view shows the focused pane and the next pane;
the terminal dimensions follow resizing and view changes.

Use all [CLI conversation commands](cli.md) inside a pane: send messages,
receive streaming replies, create/open/rename/delete threads, complete commands
with Tab, stop with Esc or `/stop`, and quit that CLI with `/exit`. Enter pressed
during a finishing conversation operation or live refresh is preserved until
the CLI is ready. Saved conversations and WebSocket updates are shared with
desktop and web too.

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
