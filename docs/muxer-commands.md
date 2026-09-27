# Shells and commands in Muxer

Keep a project terminal or command output beside your Solmu conversation.
Solmu is the default for new panes, tabs, and spaces. Shell and command panes
use the same layouts, mouse controls, naming, direct attachment, and screen reads.
They do not create conversations or accept native agent prompt commands.

## From the TUI

Press **Ctrl+b**, then **t** to split down with an interactive shell. You can
also right-click a pane and choose **Shell pane**. Exit the shell normally;
its output remains in the stopped pane until you close it or choose **Restart**.

Press **Ctrl+b**, then **!**, or choose **Run command...** in the pane menu.
Enter a command and select **Run**, or press Enter. **Cancel** or Esc closes
the form. The command opens in a new split below the source pane, using the
configured working-directory policy. Solmu's unsent input is preserved.

The command form uses `/bin/sh -c` on Linux/macOS and `cmd.exe /d /c` on Windows.
Use the appropriate shell syntax for your system. Empty commands and launch
errors keep the form open with your input intact.

![Shell and command panes beside Solmu](screenshots/muxer-commands.png)

## From scripts

Creation commands accept one launch option: `--solmu`, `--shell`,
`--command TEXT`, or `--argv JSON_ARRAY`. Omit it to use your configured default.
These commands require an existing Muxer session and preserve focus by default;
add `--focus` to select the new layout.

```sh
muxer pane split 1 --shell --direction down --session work
muxer tab create --space 1 --command 'git status' --name Status --session work
muxer space create --cwd /path/to/project --shell --name Terminal --session work
muxer pane split 1 --argv '["git","status","--short"]' --session work
```

`--argv` launches the first array element as the executable and passes the
remaining strings as individual arguments. It does not expand shell variables,
pipes, or quoting. Paths containing spaces remain one argument. For PowerShell,
an example without JSON quoting is:

```powershell
muxer pane split 1 --command 'git status' --session work
```

Use `--cwd` to override the new terminal's directory. `pane get` exposes the
resolved launch kind and arguments, process ID, and stopped status. `agent list`
contains only live Solmu panes. Direct `terminal attach` also works with shells
and commands; **Ctrl+b q** detaches without stopping them.
`pane wait` also accepts `--until shell` and `--until running` for these panes.

Launch arrays support at most 128 entries and 64 KiB of argument text.
An executable is required; NUL bytes are rejected. Failed launches leave the
existing layout and processes intact.

## Choose a default shell

Edit Muxer's `config.toml`, or edit these fields in **Settings**:

```toml
[terminal]
new_pane = "solmu" # or "shell"
shell = ""        # executable name or path, not a command line
shell_mode = "auto"
```

An empty `shell` uses `$SHELL`, then `/bin/sh`, on Linux/macOS and
`powershell.exe -NoLogo` on Windows. `shell_mode` accepts `auto`, `login`, or
`non_login`: auto starts a login shell on macOS; login adds `-l` on Unix;
non_login leaves the shell's arguments unchanged. Windows ignores login mode.
If a shell needs specific arguments, use an explicit launch array instead.

Changes apply automatically to future panes. Existing panes keep their launch
settings. See [Muxer settings](muxer-configuration.md) for file locations and
working-directory options.

## Exit, detach, and restart

Detaching leaves shell and command processes running. Closing their pane or
stopping the Muxer server terminates them. Output remains readable after a
process exits while the server is running.

After a server restart, shell panes start a new shell with their saved arguments
and directory. Command panes stay stopped with **command awaits explicit restart**;
choose **Restart**, or run `muxer pane restart ID`, to run the saved command again.
This also applies to a command that was still running when the server stopped.
Solmu panes continue to restore their saved conversations as before. Terminal
screens and shell input are not restored across a server restart.

The [automation API](muxer-automation.md) accepts an optional `launch` object in
`create_space`, `create_tab`, and `split_pane`. Examples:

```json
{"kind":"solmu"}
{"kind":"shell"}
{"kind":"shell","argv":["/bin/bash","--noprofile","--norc"]}
{"kind":"command","argv":["git","status","--short"]}
```
