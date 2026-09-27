# Solmu

An open-source autonomous agent. A Rust backend with terminal,
native desktop, and web clients that share your conversations.

[Website](https://panuhorsmalahti.github.io/solmu/) · [Client guide](docs/clients.md) · [Configuration](docs/configuration.md) · [Releases](https://github.com/panuhorsmalahti/solmu/releases)

Licensed under [MIT](LICENSE).

- Stream replies from OpenAI, Anthropic, Gemini, and other providers.
- Save conversations locally in SQLite, with automatic thread names.
- Create, open, rename, and delete threads in every client.
- See changes across clients instantly and stop responses anytime.
- Link directly to conversations in the web client.
- Customize the shared system prompt and default model in Profile.
- Choose a model per thread and work from your CLI's current folder.
- Run locally, in Docker, or in a Linux sandbox workspace.
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

Prefer individual modules? Install the [backend](docs/services.md#install),
then only the clients you need: [CLI](clients/cli/README.md#install),
[Desktop](clients/desktop/README.md#install), [Web](clients/web/README.md#install),
or [Muxer](muxer/README.md#install). [Boxer](boxer/README.md#install) is separate too.

Add your provider key to `~/.solmu/.env` (Windows: `%USERPROFILE%\.solmu\.env`),
then restart the backend service. Run `solmu`, `solmu-desktop`, or `muxer`,
or open **http://127.0.0.1:3000** for the web client. The backend serves it directly.
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

## Muxer

A Rust TUI for project spaces with multiple Solmu tabs and nested terminal panes.
Switch spaces, arrange conversations, and track working/idle/error states.
With the backend running, launch the installed `muxer` command.
Click **+ Tab** or press **Ctrl+b n** to add a tab. **Ctrl+b s** splits a pane right;
**Ctrl+b -** splits down. Drag dividers to resize, or **Ctrl+b z** to zoom.
**Ctrl+b q** detaches while panes keep running. Run `muxer` to reattach;
`muxer server stop` ends the session. Use `--session NAME` for separate sessions.
Spaces and pane layouts return after a server restart, with existing Solmu
conversations reopened. Name spaces, tabs, and panes with their right-click
menus; **Find** searches across the session. **Navigate** and searchable **Help**
make keyboard controls easy to discover.
Customize shortcuts and colors in **Settings**; configuration changes apply
automatically. [Muxer settings](docs/muxer-configuration.md).
[Local automation commands](docs/muxer-automation.md) inspect sessions, manage
layouts, send terminal input, and read pane screens without opening the TUI.
Subscribe to live session updates or wait for a pane state or matching output.
[Send prompts directly to Solmu](docs/muxer-agents.md), queue tasks in order,
wait for a specific reply, and cancel pending work while preserving terminal drafts.
[Direct terminal attachment](docs/muxer-terminals.md) opens one existing pane or
streams its live screen to scripts. Several observers can watch while one
controller owns input and size, with explicit takeover and draft-preserving detach.
[Run and use muxer](muxer/README.md).

![Solmu muxer terminal workspace](docs/screenshots/muxer.png)

## Boxer

The Rust `boxer` launcher starts Solmu or another program. All network
requests are allowed. On Linux, `--isolated --cwd /path/to/project` gives it
private processes and a filesystem view with only the workspace writable,
filtered system calls, dropped capabilities, and CPU/memory/task limits.
[Permissions and setup](docs/boxer.md).

Example (uses default permissions):

```sh
boxer --cwd /path/to/project -- solmu
```

## Docker

For agent sandboxing, Solmu recommends [Boxer](docs/boxer.md) over Docker.
Docker provides a convenient way to run the backend.

```sh
docker run --rm --env-file .env -p 127.0.0.1:3000:3000 \
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
