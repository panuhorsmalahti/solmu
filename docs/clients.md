# Solmu clients

The Android client is a native Kotlin and Jetpack Compose app. It uses the same
backend APIs and live events as the other clients; see the [Android guide](android.md).

Use `/skills` in the CLI or **Skills** in desktop and web to see the current
workspace's installed skills. Lists update automatically. See [skills](skills.md).
Use `/plugins` in CLI or Muxer, or **Plugins** in desktop, Android, and web, to see
installed [Agent Plugins](plugins.md) and any loading errors.

All clients connect to the same backend and share saved conversations.
Open [Profile](profile.md) to edit the shared system prompt and optional default
model. Every client also supports per-thread model selection.
Open [Audit](audit.md) to inspect saved tool calls across conversations.
Use [scheduled tasks](tasks.md) to run Solmu later or on a recurring schedule;
every client can show and manage them.
Provider credentials and SQLite data stay on the backend.

All clients display live and saved [tool activity](tools.md), including
arguments, results, errors, and cancellation. Web tool rows expand on click.

The CLI and desktop show **Ready** when available, with progress and errors
shown as needed.

Screenshots: [CLI](screenshots/cli.png) · [Desktop](screenshots/desktop.png) ·
[Android](screenshots/android.png) | [Web](screenshots/web.png) ·
[Muxer](screenshots/muxer.png) · [Muxer GUI](screenshots/muxer-gui.png).

| Client | Start after installation | Guide |
| --- | --- | --- |
| CLI | `solmu` | [Terminal client](cli.md) |
| Muxer (spaces and CLI tabs) | `muxer` | [Terminal workspace](muxer.md) |
| Muxer GUI (native window) | `muxer-gui` | [Graphical workspace](muxer-gui.md) |
| Desktop | `solmu-desktop` | [Desktop client](desktop.md) |
| Android | Install from Android Studio or build with Gradle | [Android client](android.md) |
| Web | Open `http://127.0.0.1:3000` | [Web client](web.md) |

These guides assume the backend is already running. Its [installer](services.md)
starts a background service; [Docker](running.md) is another option. The default backend address is `http://127.0.0.1:3000`.
Rust clients load `SOLMU_BACKEND_URL` from `.env` or the environment. Set it in
the Vite process environment for web development.
Source build instructions are in [development](development.md).
Each client README has its own install command. See
[individual component installers](releases.md#individual-components) to install
one client, the backend, or Boxer separately.

Every client supports creating, listing, opening, renaming, and deleting threads;
reading saved history; sending messages; receiving streamed replies; and seeing
provider errors. Desktop and web have a thread selector on the left and a **+**
control for new threads. CLI uses `/threads`, `/open`, and `/new`.

Threads begin as “New conversation” and get a generated name after the first
user message. A manually chosen name is preserved. See
[configuration](configuration.md) to choose the title model.

WebSocket notifications keep conversation changes synchronized across clients.
The web client uses `/threads/{id}` links for individual conversations. Stop
controls cancel a pending reply; the CLI uses `/stop` or Esc. `/exit` quits the
CLI. Closing another client leaves saved conversations on the backend.
`solmu --thread <id>` reopens a thread at startup. Muxer sessions keep their
panes running after detaching and restore layouts and conversations after a
server restart; see the [Muxer guide](muxer.md). Scripts can manage layouts
and inspect or send terminal input through [Muxer automation](muxer-automation.md).
[Native Solmu controls](muxer-agents.md) queue prompts, wait for individual
replies, and stop pending work while preserving CLI and Profile drafts.

[Direct terminal attachment](muxer-terminals.md) opens one existing pane or streams its
live screen to scripts. Several observers can watch while one controller owns
input and size, with explicit takeover and draft-preserving detach.

[Shell and command panes](muxer-commands.md) run local terminals and project commands beside
Solmu. New-pane defaults apply automatically; saved commands wait for explicit
restart after the server restarts.

## MCP

Use `/mcp` in CLI and Muxer or choose **MCP** in desktop and web to see automatically connected workspace tools. See [MCP setup](mcp.md).
