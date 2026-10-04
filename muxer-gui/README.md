# Solmu Muxer GUI

Native Rust desktop companion for Muxer, built with Iced.

![Muxer GUI](../docs/screenshots/muxer-gui.png)

## Install

Linux/macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-muxer-gui.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-muxer-gui.ps1 | iex
```

This installs Muxer GUI and its `muxer` and `solmu` runtimes from the latest
release. It assumes the Solmu backend is already running. Rerun the command to
update it manually; automatic daily updates are enabled by default.

## Run

```sh
muxer-gui
```

The app connects to the local `default` Muxer session and updates automatically.
Use the session and workspace fields to select or start another session. Set
`SOLMU_MUXER_PATH` if the Muxer executable is installed outside `PATH` and is
not next to the GUI executable.

Read the [Muxer GUI guide](../docs/muxer-gui.md) and the [Muxer guide](../docs/muxer.md).
