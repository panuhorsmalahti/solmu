# Boxer GUI

Boxer GUI is a native Rust app for launching and managing Boxer processes.
Install Boxer first; the GUI uses the same `boxer` executable, profiles, and
local session files as the CLI.

## Install

Install on Linux or macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-boxer-gui.sh | sh
```

Install on Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-boxer-gui.ps1 | iex
```

The GUI and the Boxer CLI can run at the same time. Both use
`~/.boxer/sessions` on Linux/macOS or `%USERPROFILE%\.boxer\sessions` on
Windows. Boxer GUI watches that directory for filesystem events, so launches,
stops, and completed processes appear without periodic polling. On startup it
reads the current records once to include changes made while it was closed.

Use the profile, program, and workspace fields to start a launch. The list shows
attached and detached processes started from either app, and the **Stop** action
uses the same Boxer process controls as the CLI. Set `BOXER_PATH` if the Boxer
executable is not beside `boxer-gui` or available on `PATH`.

## Build locally

From the repository root:

```sh
cargo build --locked -p solmu-boxer -p solmu-boxer-gui
cargo run --locked -p solmu-boxer-gui
```

See [Boxer setup and permissions](boxer.md) for policy details.

![Boxer GUI showing a live Boxer launch](screenshots/boxer-gui.png)
