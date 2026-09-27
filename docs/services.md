# Backend installation and background service

## Install

Linux/macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-backend.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-backend.ps1 | iex
```

Installs only `solmu-backend`, registers it for your user account, and starts it.
No administrator privileges are needed. Install your chosen
[clients separately](releases.md#individual-components), or choose the
[bundle](bundle.md). Clients assume this backend is running.

## Configure

Edit `~/.solmu/.env` (Windows: `%USERPROFILE%\.solmu\.env`). The installer
creates a template only if the file does not exist; upgrades preserve it.

```dotenv
LLM_PROVIDER=openai
OPENAI_API_KEY=your-api-key
LLM_MODEL=gpt-6-sol
LLM_TITLE_MODEL=gpt-6-luna
SOLMU_WEB_DIR=web
```

Restart the service after editing configuration. The service runs from
`~/.solmu` and saves its database there. It listens on `127.0.0.1:3000`.
The [web installer](web.md) supplies files served at that same address.
Missing web files do not prevent native clients or the API from working.

Set `SOLMU_SERVICE_DIR` before installation for another state folder.
Set `SOLMU_WEB_INSTALL_DIR` for a different bundle web folder, and set
`SOLMU_WEB_DIR` in the backend's `.env` to match. Provider variables supplied
by the service environment take precedence over `.env`.
See [all configuration options](configuration.md).

## Linux

Uses a systemd user service, `solmu-backend.service`. A running user systemd
manager is required. It starts with your login session and restarts after crashes.

```sh
systemctl --user status solmu-backend
systemctl --user restart solmu-backend
systemctl --user stop solmu-backend
journalctl --user -u solmu-backend -f
```

To keep it running after logout, enable lingering with your system administrator:
`loginctl enable-linger "$USER"`. To disable automatic startup:
`systemctl --user disable --now solmu-backend`.

## macOS

Uses the LaunchAgent `dev.solmu.backend`, installed under
`~/Library/LaunchAgents`. It starts at login and restarts after crashes.

```sh
launchctl print "gui/$(id -u)/dev.solmu.backend"
launchctl kickstart -k "gui/$(id -u)/dev.solmu.backend"
launchctl bootout "gui/$(id -u)/dev.solmu.backend"
launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/dev.solmu.backend.plist"
```

Output is in `~/.solmu/backend.log` and `backend-error.log`.
Remove its LaunchAgent plist after stopping to disable login startup.

## Windows

Uses the current user's **Solmu Backend** Task Scheduler task. This is a
background login daemon, rather than a Windows Service Control Manager service.
It runs without a visible console or administrator privileges. Its supervisor
restarts the backend; stopping the task also terminates its child processes.

```powershell
Get-ScheduledTask -TaskName 'Solmu Backend'
Stop-ScheduledTask -TaskName 'Solmu Backend'
Start-ScheduledTask -TaskName 'Solmu Backend'
```

Stop and start to restart. Output is in `%USERPROFILE%\.solmu\backend.log`
and `backend-error.log`. To disable login startup:
`Disable-ScheduledTask -TaskName 'Solmu Backend'`.

## Manual supervision

For an existing service manager, set `SOLMU_NO_SERVICE=1` before installation,
or pass `-NoService` to a downloaded PowerShell installer. This installs files
without registering or stopping a service. Run `solmu-backend` from your chosen
state folder yourself. Only backend and bundle installers manage services.
