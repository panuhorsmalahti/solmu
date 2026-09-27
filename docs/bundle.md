# Install the complete Solmu bundle

This optional installer installs the backend, CLI, desktop, web client,
Boxer, and Muxer together. It is separate from the
[individual module installers](releases.md#individual-components).

Linux/macOS (requires `unzip`):

```sh
curl -fsSL https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-bundle.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-bundle.ps1 | iex
```

The installer downloads the latest published release, verifies native and web
archive checksums, installs all clients, and starts the backend as a
[background service](services.md). It uses your account and requires no admin
privileges. Open a new terminal to pick up the installed programs on PATH.

Fill your provider key in `~/.solmu/.env` (Windows:
`%USERPROFILE%\.solmu\.env`), then restart the backend service. See
[configuration](configuration.md) for providers and models.

Choose a client:

| Client | Open |
| --- | --- |
| Terminal | `solmu` |
| Desktop | `solmu-desktop` |
| Terminal workspace | `muxer` |
| Web | `http://127.0.0.1:3000` |

`boxer --cwd /path/to/project -- solmu` launches the terminal client through
Boxer; see [sandbox permissions](boxer.md).

Native binaries go to `~/.local/bin` or `%LOCALAPPDATA%\Solmu\bin`.
Web files go to `~/.solmu/web` or `%USERPROFILE%\.solmu\web`.
Set `SOLMU_VERSION` to pin a release, `SOLMU_INSTALL_DIR` for another native
binary folder, or `SOLMU_SERVICE_DIR` for another state folder. See
[installer options](releases.md#install) and [services](services.md).

Rerun to update the bundle. The installer verifies both archives before stopping
its backend service, preserves provider configuration, and retains old hashed
web assets for existing browser tabs. You can also update one module separately.
