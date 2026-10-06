# Solmu Boxer

A Rust launcher for Solmu and other programs using native kernel controls.
Filesystem access and **all network requests are allowed by default**.

## Install

Linux/macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-boxer.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-boxer.ps1 | iex
```

Installs only `boxer`. To launch Solmu, install the
[CLI](../clients/cli/README.md#install) and use an already running backend.
Rerun the same command to update Boxer manually. Installed modules update automatically each day by default; see [update settings](../docs/configuration.md).

## Run

```sh
boxer -- solmu
boxer --cwd /path/to/project -- another-agent
```

On Linux/macOS, restrict filesystem access to your project and the system runtime:

```sh
boxer --profile solmu --cwd /path/to/project -- solmu
boxer --profile solmu --cwd /path/to/project -- solmu-backend
boxer --workspace --cwd /path/to/project --read /path/to/reference -- another-agent
```

Boxer also has profiles for installed OpenCode, Codex, and Claude Code CLIs on Linux/macOS:

```sh
boxer --profile opencode --cwd /path/to/project
boxer --profile codex --cwd /path/to/project
boxer --profile claude-code --cwd /path/to/project
```

Each profile keeps its own login and settings under `~/.boxer/profiles/`.
Sign in when you first run it. See [agent profiles](../docs/boxer.md#run-claude-code-or-codex)
for permissions and installation notes.

The Solmu profile also filters the environment and sets the backend's default
workspace. Tools run in the backend, so launch the backend inside Boxer to protect
their file access. A boxed client uses its existing backend's permissions.
Keep reusable grants in an explicit JSON policy and inspect them with
`boxer --policy /path/to/boxer.json --cwd /path/to/project --print-policy -- solmu`.
Check that your OS can apply the permissions before launching an agent with
`boxer --profile solmu --cwd /path/to/project --check`. This starts only a
short-lived Boxer probe and reports readiness as JSON.
For an offline command on Linux/macOS, add `--network deny`, for example:

```sh
boxer --workspace --network deny --cwd /path/to/project -- /bin/sh ./script.sh
```

This also blocks localhost and Unix socket connections, and applies to
subprocesses. Solmu's clients and backend need networking to communicate.
Native Windows currently rejects filesystem and network restrictions.

On Linux, choose container-style isolation with a private process tree and
filesystem view, a writable workspace, read-only system files, filtered system
calls, and CPU/memory/task limits:

```sh
sudo apt install bubblewrap
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  boxer --isolated --cwd /path/to/project -- solmu
```

Networking stays allowed. This uses the host kernel.

On Linux, restrict destinations with `--isolated --network proxy`. Add exact
hosts with `--allow-host api.openai.com`, forward an existing backend with
`--allow-local 127.0.0.1:3000`, or expose a boxed backend with `--publish 3000`.
See [network routes and Solmu examples](../docs/boxer.md#choose-allowed-network-destinations-on-linux).
This routes traffic through a private network and preserves streaming and
WebSocket support. Provider keys remain visible to the agent.

Running `boxer` by itself starts the Solmu terminal client (`solmu`).
It is equivalent to `boxer -- solmu`. To launch the backend or another
program, name it after `--`, for example `boxer -- solmu-backend`.

Use `--read-only` on Linux/macOS to deny filesystem
writes; Windows supports process-tree containment only.

See [permissions, platform requirements, and usage](../docs/boxer.md).
