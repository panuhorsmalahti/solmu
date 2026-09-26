# Web client

With the backend running, use `npm ci` and `npm run dev:web` from the repository
root. Open `http://127.0.0.1:5173`. Set `SOLMU_BACKEND_URL` in the Vite process
environment to change the backend address (default `http://127.0.0.1:3000`).

A new conversation is created on startup. The sidebar lists saved threads;
click a thread to resume it or **+** to create one. The title gets an automatic
name after the first message. Edit it and click the check icon to rename it.
The trash icon deletes the thread and all its messages.

Type a message and press Enter or the send arrow. Shift+Enter adds a newline.
Replies stream into the conversation and are saved when complete. Thread
changes and sending are disabled while a reply is processing. Provider errors
appear above the composer; user messages remain saved.

Use **Refresh conversations** to read changes made in another client.
The sidebar also refreshes automatically to show generated titles.

For a production build, run `npm run build:web`. Serve `clients/web/dist` with
a reverse proxy that sends `/api` to the backend on the same origin. Keep all
provider credentials on the backend. The marketing website on GitHub Pages
is separate from this client and does not host a backend or conversations.
