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

Create a policy scaffold with `boxer policy init` and print its JSON Schema with
`boxer policy schema`. Inspect, validate, or compare policy files with `boxer policy profiles`,
`boxer policy validate FILE`, `boxer policy show FILE`, and
`boxer policy diff BEFORE AFTER`. See the
[policy guide](../docs/boxer.md#policy-paths-and-fields).

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

Boxer also has profiles for installed Pi, OpenCode, Codex, and Claude Code CLIs on Linux/macOS:

```sh
boxer --profile opencode --cwd /path/to/project
boxer --profile codex --cwd /path/to/project
boxer --profile claude-code --cwd /path/to/project
boxer --profile pi --cwd /path/to/project
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

On Linux, restrict destinations with `--isolated --network proxy`. Choose a
built-in host set with `--network-profile minimal|developer|claude-code|codex|opencode|enterprise` or add exact
hosts or wildcard patterns with `--allow-host api.openai.com`. Forward an existing backend with
`--allow-local 127.0.0.1:3000`, or expose a boxed backend with `--publish 3000`.
See [network routes and Solmu examples](../docs/boxer.md#choose-allowed-network-destinations-on-linux).
This routes traffic through a private network and preserves streaming and
WebSocket support. Provider keys remain visible to the agent.
An HTTP CONNECT proxy can be configured with `--upstream-proxy` or
`BOXER_UPSTREAM_PROXY`; exact and wildcard bypasses are available with
`--upstream-bypass`. See the network route guide above.

Running `boxer` by itself starts the Solmu terminal client (`solmu`).
It is equivalent to `boxer -- solmu`. To launch the backend or another
program, name it after `--`, for example `boxer -- solmu-backend`.

Use `--read-only` on Linux/macOS to deny filesystem
writes; Windows supports process-tree containment only.

See [permissions, platform requirements, and usage](../docs/boxer.md).

Use `boxer why --path PATH --op read|write` with the policy options you plan to
run to see how Boxer resolves that filesystem access. See the
[path explanation guide](../docs/boxer.md#explain-a-path-decision).

Sign instruction files with `boxer trust keygen` and `boxer trust sign`, then
require signatures at launch with `--trust-key` and `--verify` or a signed
`--trust-policy`. See the
[instruction trust guide](../docs/boxer.md#verify-trusted-instruction-files).

Store credentials with `boxer credential set NAME` and load them into an agent
with `--env-credential NAME`. Boxer uses the OS credential store and prompts
without echoing the value. The value is available to the agent process. See the
[credential guide](../docs/boxer.md#store-credentials-for-an-agent).

On Linux, `--credential openai|anthropic|gemini|github|gitlab` keeps the stored provider key outside
the agent and uses a local proxy. Policy files can also define custom HTTPS
credential routes. These features require `--isolated --network proxy`.
See the [credential guide](../docs/boxer.md#store-credentials-for-an-agent).

Use `--runtime-group node|python|rust|go` with `--workspace` or `--isolated` to
grant read access to detected toolchains. See the
[runtime group guide](../docs/boxer.md#give-access-to-an-installed-toolchain).

Add `--rollback` to save a workspace before and after a run. Review or restore
those snapshots with `boxer rollback list`, `boxer rollback show <session-id>`,
and `boxer rollback restore <session-id>`. See the
[rollback guide](../docs/boxer.md#review-and-restore-a-session).
Preview or prune old snapshots with `boxer rollback cleanup --older-than DAYS`
or `--keep COUNT`; shared content remains for retained sessions.
Use `boxer rollback audit list|show|verify` to inspect and validate the local
session event chain. It covers recorded lifecycle and snapshot changes, not all
kernel or network activity. See the [audit guide](../docs/boxer.md#audit-a-session).
