# Web client

With the backend running, use `npm ci` and `npm run dev:web` from the repository
root. Open `http://127.0.0.1:5173`. Set `SOLMU_BACKEND_URL` in the Vite process
environment to change the backend address (default `http://127.0.0.1:3000`).

A new conversation is created when opening the root URL. Each thread has a
linkable `/threads/{id}` URL; opening that URL loads the existing conversation.
Browser Back/Forward switches between visited threads. The sidebar lists saved threads;
click a thread to resume it or **+** to create one. The title gets an automatic
name after the first message. Edit it and click the check icon to rename it.
The trash icon deletes the thread and all its messages.

Type a message and press Enter or the send arrow. Shift+Enter adds a newline.
Replies stream into the conversation and are saved when complete. Thread
changes and sending are disabled while a reply is processing. Provider errors
appear above the composer; user messages remain saved.

**Stop** cancels the current response, keeping your user message and discarding
the unfinished reply. Changes from other clients and generated titles arrive
automatically over WebSockets, with reconnect after a lost connection.

For a production build, run `npm run build:web`. Serve `clients/web/dist` with
a reverse proxy that sends `/api` to the backend on the same origin, including
WebSocket upgrades for `/api/v1/events`. Serve `index.html` for `/threads/*`
so conversation links and reloads work. Keep all
provider credentials on the backend. The marketing website on GitHub Pages
is separate from this client and does not host a backend or conversations.

![Solmu client screenshot](screenshots/web.png)
