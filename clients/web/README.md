# Solmu web

A React and TypeScript client with shadcn/ui components.

## Run

Start the backend with `cargo run -p solmu-backend`, then from the repository root:

```sh
npm ci
npm run dev:web
```

Open `http://127.0.0.1:5173`. The development server connects to the backend at
`http://127.0.0.1:3000`; set `SOLMU_BACKEND_URL` in the shell running Vite to
choose another backend. Provider keys stay in the backend's `.env`.

## Use

Select a thread in the left sidebar to load its history. Click the **+** icon
to create a new thread. Edit the title and click the check icon to rename it;
the trash icon deletes the thread and its messages.

Type a message and press Enter or the send arrow. Shift+Enter adds a new line.
Replies stream as they arrive. Threads get an automatic name after the first
message unless you chose a name manually. Changes from other clients appear
automatically over WebSockets. **Stop** cancels a response. Each conversation
has a linkable `/threads/{id}` URL. Errors appear above the composer.

## Build

```sh
npm run lint:web
npm run build:web
```

The static build is in `clients/web/dist`. In production, serve it on the same
origin as the backend and proxy `/api` to Solmu, including WebSocket upgrades.
Serve `index.html` for `/threads/*`. The web client does not embed
provider keys or connect directly to LLM providers.

See [web usage](../../docs/web.md).
