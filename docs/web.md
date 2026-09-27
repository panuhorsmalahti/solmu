# Web client

## Run the published web client

After [installation](releases.md#install), configure your provider key in
`.env` and run `solmu-backend`. Download `solmu-v<VERSION>-web.zip` from
[GitHub Releases](https://github.com/panuhorsmalahti/solmu/releases) and extract
it into a folder named `solmu-web`. The installer installs native binaries;
the web archive is a separate download.

Serve the extracted files with a web server that proxies `/api/*` to
`http://127.0.0.1:3000`, including WebSocket upgrades, and serves `index.html`
for conversation and Profile URLs. For example, with
[Caddy installed](https://caddyserver.com/docs/install), save this `Caddyfile`
beside the `solmu-web` folder:

```caddyfile
http://127.0.0.1:8080 {
    handle /api/* {
        reverse_proxy 127.0.0.1:3000
    }
    handle {
        root * ./solmu-web
        try_files {path} /index.html
        file_server
    }
}
```

In another terminal, run `caddy run --config Caddyfile` from that directory,
then open `http://127.0.0.1:8080`. This follows Caddy's
[single-page app configuration](https://caddyserver.com/docs/caddyfile/patterns#single-page-apps-spas).
If your backend uses another address, change `reverse_proxy` accordingly.
Provider keys stay on the backend. Rust and Node.js are not needed to use
the published web files. The marketing website on GitHub Pages is separate
from this client and does not host a backend or conversations.

## Use

A new conversation is created when opening the root URL. Each thread has a
linkable `/threads/{id}` URL; opening that URL loads the existing conversation.
Browser Back/Forward switches between visited threads. The sidebar lists saved threads;
click a thread to resume it or **+** to create one. The title gets an automatic
name after the first message. Edit it and click the check icon to rename it.
The trash icon deletes the thread and all its messages.

Repeated **+** clicks reuse the current empty conversation until you send a
message. Open **Profile** above Conversations, or click **solmu**, to edit your
shared system prompt and optional default model. The page shows when the prompt
was last edited, and the empty Model field displays the actual backend default.
Use the model button in a conversation to override its model or return to the
default. OpenAI and Anthropic show named choices; custom model IDs also work.
Profile changes appear live while preserving unsaved edits.
See [Profile](profile.md) and [workspaces](workspaces.md).

Type a message and press Enter or the send arrow. Shift+Enter adds a newline.
Replies stream into the conversation and are saved when complete. Thread
changes and sending are disabled while a reply is processing. Provider errors
appear above the composer; user messages remain saved.

**Stop** cancels the current response, keeping your user message and discarding
the unfinished reply. Changes from other clients and generated titles arrive
automatically over WebSockets, with reconnect after a lost connection.

For source builds and web development, see [development](development.md).

![Solmu client screenshot](screenshots/web.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](tools.md) for usage and limits.
