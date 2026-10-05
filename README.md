# Solmu

An open-source autonomous agent. A Rust backend with terminal,
native desktop, Android, iOS, and web clients that share your conversations.

[Website](https://panuhorsmalahti.github.io/solmu/) · [Docs](https://panuhorsmalahti.github.io/solmu/docs/) · [Client guide](docs/clients.md) · [Configuration](docs/configuration.md) · [Releases](https://github.com/panuhorsmalahti/solmu/releases)

Licensed under [MIT](LICENSE).

- Stream replies from OpenAI, Anthropic, Gemini, and other providers.
- Save conversations locally in SQLite, with automatic thread names.
- Create, open, rename, and delete threads in every client.
- See changes across clients instantly and stop responses anytime.
- Link directly to conversations in the web client.
- Customize the shared system prompt and default model in Profile.
- Inspect every saved tool call and the last 24 hours' prompt cache hit rate in Audit. See [Audit](docs/audit.md).
- Schedule Solmu to run once later or on a recurring cron schedule, with a saved conversation and run history for each task. See [Tasks](docs/tasks.md).
- Use project skills discovered automatically from `.agents/skills/`.
- Connect workspace MCP servers to use additional tools and inspect their live status in every client. See [MCP setup](docs/mcp.md).
- Trigger agent conversations from GitHub and other services with authenticated [webhooks](docs/webhooks.md).
- Install Agent Plugins to bring skills and MCP tools into a workspace together. See [plugins](docs/plugins.md).
- Choose a model per thread and work from your CLI's current folder.
- Run locally, in Docker, or in a Linux sandbox workspace.
- Manage Solmu agent sandboxes in Kubernetes with [Congregator](docs/congregator.md) and NVIDIA OpenShell.
- Group Solmu terminal tabs into project spaces with nested panes and mouse controls.
- Read, edit, and search workspace files and run Bash commands with saved tool results.

## Install the bundle

Linux/macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-bundle.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install-bundle.ps1 | iex
```

The optional bundle installs the backend, every client, Boxer, and Muxer from
the latest release, verifies checksums, and starts the backend in the background.
See [bundle setup](docs/bundle.md) and [service management](docs/services.md).
Installed modules update daily by default. Rerun each module's installer once
to register an existing installation; set `SOLMU_AUTO_UPDATE=false` in
`~/.solmu/.env` to disable updates. Docker images update by pulling a new image.
See [update details](docs/releases.md).

Prefer individual modules? Install the [backend](docs/services.md#install),
then only the clients you need: [CLI](clients/cli/README.md#install),
[Desktop](clients/desktop/README.md#install), [Android](clients/android/README.md),
[iOS](clients/ios/README.md), [Web](clients/web/README.md#install), or
[Muxer](muxer/README.md#install).
[Boxer](boxer/README.md#install) is separate too.

Add your provider key to `~/.solmu/.env` (Windows: `%USERPROFILE%\.solmu\.env`),
then restart the backend service. Run `solmu`, `solmu-desktop`, or `muxer`,
open **http://127.0.0.1:3000** for the web client, or install the signed Android
APK from [GitHub Releases](https://github.com/panuhorsmalahti/solmu/releases).
You can also build the Android app from [Android Studio](clients/android/README.md)
or build the native
[iOS app](clients/ios/README.md) with Xcode on a Mac. The backend serves the web client directly.
See [configuration](docs/configuration.md) for model choices.

## From source

Install Rust and Node.js 24 (Node is needed only for the web client).
Copy `.env.example` to `.env` if you have not created it, then fill your key.
From the repository root:

```sh
cargo run -p solmu-backend
```

In another terminal, choose a client:

```sh
cargo run -p solmu-cli
cargo run -p solmu-desktop
```

For the web client:

```sh
npm ci
npm run dev:web
```

Open `http://127.0.0.1:5173`. Provider keys stay on the backend.

## Clients

### Terminal

A Rust TUI with streamed replies, an animated spinner, Tab command completion,
`/threads`, `/open`, `/new`, and `/exit`. Esc or `/stop` cancels a reply.
[Run and use the CLI](clients/cli/README.md).

![Solmu terminal client](docs/screenshots/cli.png)

### Desktop

A native Rust app built with Iced. Select conversations in the left sidebar,
create one with **+**, and stop a response with **Stop**.
[Run and use the desktop client](clients/desktop/README.md).

![Solmu desktop client](docs/screenshots/desktop.png)

### Web

React with shadcn/ui. Saved threads, live updates, response controls, and
linkable `/threads/{id}` conversation pages.
[Run and use the web client](clients/web/README.md).

![Solmu web client](docs/screenshots/web.png)

### Android

A native Kotlin and Jetpack Compose app. It connects to the same backend and
provides conversations, streaming, Profile, Audit, Tasks, and workspace tools.
[Build and run the Android client](clients/android/README.md).

<a href="docs/screenshots/android.png"><img src="docs/screenshots/android.png" alt="Solmu Android client" width="240"></a>

### iOS

A native SwiftUI app for iPhone. Connects to the same backend and includes
conversations, streaming, Profile, Audit, Tasks, Skills, MCP, Plugins, and
webhooks. Build and run with Xcode on a Mac.
[Build the iOS client](clients/ios/README.md).

<a href="docs/screenshots/ios.png"><img src="docs/screenshots/ios.png" alt="Solmu iOS client" width="240"></a>

### Congregator

Manage Solmu agents running in isolated Kubernetes sandboxes with a dedicated
web dashboard. [Install Congregator](docs/congregator.md).

![Congregator cloud dashboard](docs/screenshots/congregator.png)

## Muxer

A Rust TUI for project spaces, tabs, and nested terminal panes. Keep Solmu
conversations, interactive shells, and project commands together. With the backend
running, launch `muxer`.

**Arrange your workspace.** Click **+ Tab** or press **Ctrl+b n** to add a tab.
Use **Ctrl+b s** to split right, **Ctrl+b -** to split down, and **Ctrl+b z** to
zoom. Drag dividers to resize. Name spaces, tabs, and panes with their context
menus; **Find**, **Navigate**, and searchable **Help** make navigation easy.

**Keep sessions running.** **Ctrl+b q** detaches without stopping panes.
Run `muxer` to reattach, or `muxer server stop` to end the session.
Use `--session NAME` for separate sessions. Saved layouts and Solmu conversations
return after a server restart.

**Run local commands.** **Ctrl+b t** opens a shell pane; **Ctrl+b !** opens the
command form. Their output remains readable after exit. Choose **Restart** to
run a saved command again. [Shells and commands](docs/muxer-commands.md).

**Make it yours.** Change shortcuts, colors, sidebar options, and new-pane defaults
in **Settings**. Changes apply automatically. [Muxer settings](docs/muxer-configuration.md).

**Automate work.** [Local commands](docs/muxer-automation.md) manage layouts,
send terminal input, read screens, and subscribe to live changes.
[Native Solmu prompts](docs/muxer-agents.md) queue tasks and wait for saved replies
while preserving terminal drafts. [Direct attachment](docs/muxer-terminals.md)
opens one existing pane, with one controller and multiple observers.

[Run and use Muxer](muxer/README.md).

![Solmu muxer terminal workspace](docs/screenshots/muxer.png)

### Muxer GUI

Prefer a window? [Muxer GUI](muxer-gui/README.md) is a standalone native Rust
workspace app; it does not need the Muxer terminal app or server. It shares
local session files with Muxer. Choose Solmu to show the shared native Solmu
desktop app, or Terminal to use a shell and other command-line clients such as
Claude Code.

![Solmu Muxer GUI](docs/screenshots/muxer-gui.png)

## Boxer

The Rust `boxer` launcher starts Solmu or another program. All network
requests are allowed by default. On Linux/macOS, `--profile solmu` restricts file access
to a writable project and read-only runtime files. Add explicit read/write grants
or reuse a JSON policy; preview permissions with `--print-policy` or check that
your OS can enforce them with `--check` before launching your agent.
Profiles for Codex and Claude Code launch those installed CLIs with separate
login homes and the same project access.
Use `--network deny` for offline commands, including their subprocesses.
On Linux, `--isolated --network proxy` permits only explicit remote and local
routes; publish the boxed backend on port 3000 to use normal client settings.
Launch the backend inside Boxer to protect its tool calls.
On Linux, `--isolated --cwd /path/to/project` also gives it
private processes and a filesystem view with only the workspace writable,
filtered system calls, dropped capabilities, and CPU/memory/task limits.
[Permissions and setup](docs/boxer.md).

Example (Linux/macOS backend workspace permissions):

```sh
boxer --profile solmu --cwd /path/to/project -- solmu-backend
```

## Docker

For agent sandboxing, Solmu recommends [Boxer](docs/boxer.md) over Docker.
Docker provides a convenient way to run the backend.

```sh
docker run --rm --env-file .env -p 127.0.0.1:3000:3000 \
  -p 127.0.0.1:3001:3001 \
  -v solmu-data:/data ghcr.io/panuhorsmalahti/solmu:latest
```

Or build with `docker build -t solmu .` and use `solmu` as the image name.
The volume keeps conversations across container restarts.
[Docker setup](docs/running.md).

## Development

The backend exposes REST APIs, SSE responses, and WebSocket notifications.
All clients have scoped end-to-end tests. GitHub Actions builds, lints, and tests
on Linux, Windows, and macOS, publishes the website and Docker image, and offers
daily production releases when there are new commits, plus a manual release workflow.

[API](docs/api.md) · [Build and test](docs/development.md) · [Release workflow](docs/releases.md)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](docs/tools.md) for usage and limits.

## Skills

Install skill folders with `SKILL.md` under your workspace's `.agents/skills/`.
Solmu discovers them automatically and reads relevant instructions as needed.
Use `/skills` in the terminal or **Skills** in desktop and web to see the live
catalog. [Install and use skills](docs/skills.md).
