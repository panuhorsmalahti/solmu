# Solmu clients

All clients connect to the same backend and share saved conversations.
Open [Profile](profile.md) to edit the shared system prompt and optional default
model. Every client also supports per-thread model selection.
Provider credentials and SQLite data stay on the backend.

All clients display live and saved [tool activity](tools.md), including
arguments, results, errors, and cancellation. Web tool rows expand on click.

The CLI and desktop show **Ready** when available, with progress and errors
shown as needed.

Screenshots: [CLI](screenshots/cli.png) · [Desktop](screenshots/desktop.png) ·
[Web](screenshots/web.png) · [Muxer](screenshots/muxer.png).

| Client | Start from repository root | Guide |
| --- | --- | --- |
| CLI | `cargo run -p solmu-cli` | [Terminal client](cli.md) |
| Muxer (spaces and CLI tabs) | `cargo build -p solmu-cli`, then `cargo run -p solmu-muxer` | [Terminal workspace](muxer.md) |
| Desktop | `cargo run -p solmu-desktop` | [Desktop client](desktop.md) |
| Web | `npm ci`, then `npm run dev:web` | [Web client](web.md) |

Start the backend first with `cargo run -p solmu-backend`, or use
[Docker](running.md). The default backend address is `http://127.0.0.1:3000`.
Rust clients load `SOLMU_BACKEND_URL` from `.env` or the environment. Set it in
the Vite process environment for web development.

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
`solmu-cli --thread <id>` reopens a thread at startup. Muxer sessions keep their
panes running after detaching and restore layouts and conversations after a
server restart; see the [Muxer guide](muxer.md). Scripts can manage layouts
and inspect or send terminal input through [Muxer automation](muxer-automation.md).
