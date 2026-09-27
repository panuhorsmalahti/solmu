# Solmu web

A React and TypeScript client with shadcn/ui components.

## Run

After using the [install script](../../docs/releases.md#install), configure
your provider key in `.env` and start the installed backend:

```sh
solmu-backend
```

Download `solmu-v<VERSION>-web.zip` from
[GitHub Releases](https://github.com/panuhorsmalahti/solmu/releases) and extract
it into `solmu-web`. Serve these files with a web server that proxies `/api`
to the backend at `http://127.0.0.1:3000`. Follow the
[web setup guide](../../docs/web.md#run-the-published-web-client) for a complete
local example, then open `http://127.0.0.1:8080`.

The install script installs the backend and native clients; the web archive
is downloaded separately. Provider keys stay in the backend's `.env`.

## Use

Select a thread in the left sidebar to load its history. Click the **+** icon
to create a new thread. Edit the title and click the check icon to rename it;
the trash icon deletes the thread and its messages.

Type a message and press Enter or the send arrow. Shift+Enter adds a new line.
Replies stream as they arrive. Threads get an automatic name after the first
message unless you chose a name manually. Changes from other clients appear
automatically over WebSockets. **Stop** cancels a response. Each conversation
has a linkable `/threads/{id}` URL. Errors appear above the composer.

**Profile**, above Conversations, or clicking **solmu** opens `/profile`.
Edit the shared system prompt and optional default model, and see the prompt's
last edit time. An empty Model field shows the actual backend default. Use the
conversation's model button to choose a model for that thread. Repeated **+**
clicks reuse the empty thread until you send a message. Profile updates appear
automatically and preserve unsaved edits. See [Profile](../../docs/profile.md).

See [web usage](../../docs/web.md).
For source builds, see [development](../../docs/development.md).

![Solmu client screenshot](../../docs/screenshots/web.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](../../docs/tools.md) for usage and limits.
